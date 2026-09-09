#!/usr/bin/env bash
# ci-phases.sh -- per-phase LOCAL CI gate aggregator.
#
# Runs every gate in ops/ci/gates/*.sh. Each gate prints a final line of the
# form:  GATE <name>: PASS|FAIL|PENDING ...
# We capture that line, tally results, and print a summary table.
#
# Exit policy:
#   - exit 1 for any failed, pending, or unrecognized gate result.
#   - exit 0 only when every gate exits zero and its final line is its own PASS.
#
# Modes:
#   ci-phases.sh           run all gates, print summary.
#   ci-phases.sh --list    list discovered gates (no execution).
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/.." && pwd)"
GATES_DIR="${ROOT}/ops/ci/gates"
[[ $# == 0 || ( $# == 1 && $1 == --list ) ]] || {
  echo 'usage: scripts/ci-phases.sh [--list]' >&2; exit 2;
}

discover_gates() {
  # Newline-delimited, sorted, *.sh only (README.md is skipped naturally).
  if [ -d "${GATES_DIR}" ]; then
    find "${GATES_DIR}" -maxdepth 1 -type f -name '*.sh' | sort
  fi
}

gate_paths="$(discover_gates)" || exit 1
if [ "${1:-}" = "--list" ]; then
  echo "Discovered phase gates in ops/ci/gates/:"
  found=0
  while IFS= read -r g; do
    [ -z "${g}" ] && continue
    found=1
    printf '  - %s\n' "$(basename "${g}" .sh)"
  done <<EOF
${gate_paths}
EOF
  [ "${found}" -eq 0 ] && echo "  (none found)"
  exit 0
fi

# Collect gates into an array.
gates=()
while IFS= read -r g; do
  [ -z "${g}" ] && continue
  gates+=("${g}")
done <<EOF
${gate_paths}
EOF

if [ "${#gates[@]}" -eq 0 ]; then
  echo "ci-phases: no gates found in ${GATES_DIR}" >&2
  exit 1
fi

# Per-gate results, kept as parallel newline-delimited tallies.
names=()
statuses=()
n_pass=0
n_fail=0
n_pending=0
n_unknown=0

for g in "${gates[@]}"; do
  name="$(basename "${g}" .sh)"
  echo "============================================================"
  echo ">>> running gate: ${name}"
  echo "============================================================"

  # Capture the complete result so an earlier or nested PASS cannot qualify it.
  out="$(bash "${g}" 2>&1)"
  rc=$?
  printf '%s\n' "${out}"

  gate_line="${out##*$'\n'}"
  status=""
  if [[ $gate_line == "GATE ${name}: "* ]]; then
    result="${gate_line#"GATE ${name}: "}"
    case "$result" in
      PASS|PASS[[:space:]]*) status=PASS ;;
      FAIL|FAIL[[:space:]]*) status=FAIL ;;
      PENDING|PENDING[[:space:]]*) status=PENDING ;;
    esac
  fi
  if [[ $status == PASS && $rc != 0 ]]; then
    echo "ci-phases: ${name} reported PASS but exited ${rc}" >&2
    status=FAIL
  fi

  case "${status}" in
    PASS)
      n_pass=$((n_pass + 1))
      ;;
    PENDING)
      n_pending=$((n_pending + 1))
      ;;
    FAIL)
      n_fail=$((n_fail + 1))
      ;;
    *)
      # No recognizable GATE line, or unexpected status -> treat as a failure
      # so we never silently pass when a gate misbehaves.
      status="UNKNOWN(rc=${rc})"
      n_unknown=$((n_unknown + 1))
      ;;
  esac

  names+=("${name}")
  statuses+=("${status}")
done

# Summary table.
echo
echo "============================================================"
echo "PHASE GATE SUMMARY"
echo "============================================================"
printf '  %-22s %s\n' "GATE" "STATUS"
printf '  %-22s %s\n' "----" "------"
i=0
while [ "${i}" -lt "${#names[@]}" ]; do
  printf '  %-22s %s\n' "${names[$i]}" "${statuses[$i]}"
  i=$((i + 1))
done
echo "------------------------------------------------------------"
printf '  totals: PASS=%d  PENDING=%d  FAIL=%d  UNKNOWN=%d  (of %d gates)\n' \
  "${n_pass}" "${n_pending}" "${n_fail}" "${n_unknown}" "${#names[@]}"
echo "============================================================"

if [ "${n_pending}" -gt 0 ]; then
  echo "${n_pending} gate(s) PENDING: required proof did not complete."
fi

if [ "${n_fail}" -gt 0 ] || [ "${n_unknown}" -gt 0 ] || [ "${n_pending}" -gt 0 ]; then
  echo "RESULT: FAIL ($((n_fail + n_unknown + n_pending)) gate(s) failed/pending/unknown)."
  exit 1
fi

echo "RESULT: OK (every gate passed)."
exit 0
