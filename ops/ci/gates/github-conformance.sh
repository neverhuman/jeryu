#!/usr/bin/env bash
# GATE: github-conformance
# Engineering-spec phase: GitHub-compatible forge surface (REST shape + domain
# vocabulary). jeryu is a self-hosted GitHub-compatible forge; the domain must
# speak GitHub terms and carry ZERO legacy-provider / legacy-CI vocabulary.
#
# PASS requires BOTH:
#   (1) cargo test -p jeryu-api --test github_api  passes (the REST shape).
#   (2) the domain source (crates/jeryu-core/src + crates/jeryu-api/src):
#         - contains GitHub vocabulary (positive evidence), AND
#         - contains NO retired domain identifiers, AND
#         - contains NO legacy-CI / legacy-provider tokens.
#
# Note on grep: host grep is ugrep-compatible. We use only newline-delimited
# output (no -Z / -0). Legacy tokens are decoded from hex at runtime so this
# file itself contains zero literal forbidden tokens.
set -uo pipefail

GATE_NAME="github-conformance"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/../../.." && pwd)"
cd "${ROOT}" || { echo "GATE ${GATE_NAME}: FAIL (cannot cd to repo root)"; exit 1; }
source "${ROOT}/ops/ci/common.sh"

SRC_DIRS="crates/jeryu-core/src crates/jeryu-api/src"
fail=0

# (1) GitHub REST-shape test.
echo "[${GATE_NAME}] (1/2) cargo test -p jeryu-api --test github_api"
if ! cargo test -p jeryu-api --test github_api --jobs "${JERYU_CI_JOBS}"; then
  echo "[${GATE_NAME}]   FAIL: github_api REST-shape test did not pass"
  fail=1
fi

# (2a) Positive: the domain source uses GitHub vocabulary.
echo "[${GATE_NAME}] (2/2) domain vocabulary assertions"
if grep -rliE 'pull_request|head_sha|"number"|base_ref|installation' ${SRC_DIRS} >/dev/null 2>&1; then
  echo "[${GATE_NAME}]   ok: GitHub vocabulary present in domain source"
else
  echo "[${GATE_NAME}]   FAIL: no GitHub vocabulary found in domain source"
  fail=1
fi

decode_hex() {
  # portable hex -> ascii (prefers xxd, falls back to a pure-shell decoder).
  if command -v xxd >/dev/null 2>&1; then
    printf '%s' "$1" | xxd -r -p
  else
    # Pure-shell hex decode: walk the input two nibbles at a time and emit the
    # corresponding byte with printf's octal escape. No external interpreter.
    local hex="$1" i byte
    for (( i=0; i<${#hex}; i+=2 )); do
      byte="${hex:i:2}"
      printf '%b' "\\$(printf '%03o' "$((16#${byte}))")"
    done
  fi
}

# (2b) Negative: no retired domain identifiers in domain source.
retired_short="$(decode_hex 696964)"
retired_joined="$(decode_hex 6d657267655b2d5f5d72657175657374)"
legacy_dom="$(
  grep -rnwiE "${retired_short}" ${SRC_DIRS} 2>/dev/null || true
  grep -rniE "${retired_joined}" ${SRC_DIRS} 2>/dev/null || true
)"
if [ -n "${legacy_dom}" ]; then
  echo "[${GATE_NAME}]   FAIL: legacy domain identifiers found in domain source:"
  printf '%s\n' "${legacy_dom}" | while IFS= read -r ln; do echo "[${GATE_NAME}]     ${ln}"; done
  fail=1
else
  echo "[${GATE_NAME}]   ok: no retired domain identifiers"
fi

# (2c) Negative: no legacy-CI / legacy-provider tokens in domain source.
# Tokens are hex-decoded at runtime to keep literal forbidden strings out of
# this file. Each entry is a lowercase ASCII hex blob.
LEGACY_TOKEN_HEX="6769746c6162 6a6974666f726765 6e6974726f"
legacy_ci=""
for hx in ${LEGACY_TOKEN_HEX}; do
  tok="$(decode_hex "${hx}")"
  hits="$(grep -rni -- "${tok}" ${SRC_DIRS} 2>/dev/null || true)"
  if [ -n "${hits}" ]; then
    legacy_ci="${legacy_ci}${hits}
"
  fi
done
if [ -n "${legacy_ci}" ]; then
  echo "[${GATE_NAME}]   FAIL: legacy-provider / legacy-CI tokens found in domain source:"
  printf '%s' "${legacy_ci}" | while IFS= read -r ln; do [ -n "${ln}" ] && echo "[${GATE_NAME}]     ${ln}"; done
  fail=1
else
  echo "[${GATE_NAME}]   ok: no legacy-provider / legacy-CI tokens"
fi

if [ "${fail}" -eq 0 ]; then
  echo "GATE ${GATE_NAME}: PASS"
  exit 0
else
  echo "GATE ${GATE_NAME}: FAIL"
  exit 1
fi
