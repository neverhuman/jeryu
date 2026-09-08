#!/usr/bin/env bash
# ci-phases.sh -- per-phase LOCAL CI gate aggregator.
#
# Runs every gate in ops/ci/gates/*.sh. Each gate prints a final line of the
# form:  GATE <name>: PASS|FAIL|PENDING ...
# We capture that line, tally results, and print a summary table.
#
# Exit policy:
#   - exit 1 if ANY gate FAILs (or a gate produced no recognizable GATE line).
#   - PENDING gates do NOT fail the run, but are reported distinctly and are
#     never hidden.
#   - exit 0 only when every gate is PASS or PENDING.
#
# Modes:
#   ci-phases.sh           run all gates, print summary.
#   ci-phases.sh --list    list discovered gates (no execution).
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "${HERE}/.." && pwd)"
GATES_DIR="${ROOT}/ops/ci/gates"

discover_gates() {
  # Newline-delimited, sorted, *.sh only (README.md is skipped naturally).
  if [ -d "${GATES_DIR}" ]; then
    find "${GATES_DIR}" -maxdepth 1 -type f -name '*.sh' | sort
  fi
}

if [ "${1:-}" = "--list" ]; then
  echo "Discovered phase gates in ops/ci/gates/:"
  found=0
  while IFS= read -r g; do
    [ -z "${g}" ] && continue
    found=1
    printf '  - %s\n' "$(basename "${g}" .sh)"
  done <<EOF
$(discover_gates)
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
$(discover_gates)
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

  # Run the gate, streaming its output to the console while also capturing it
  # so we can parse the final GATE line. We tolerate a nonzero exit (FAIL).
  out="$(bash "${g}" 2>&1)"
  rc=$?
  printf '%s\n' "${out}"

  # Extract the last line that starts with "GATE <name>:".
  gate_line="$(printf '%s\n' "${out}" | grep -E '^GATE ' | tail -n 1)"
  status="$(printf '%s\n' "${gate_line}" | sed -nE 's/^GATE [^:]+: ([A-Z]+).*/\1/p')"

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
  echo "NOTE: ${n_pending} gate(s) PENDING -- live capability still to be built."
  echo "      PENDING does not fail the run, but is reported above, not hidden."
fi

if [ "${n_fail}" -gt 0 ] || [ "${n_unknown}" -gt 0 ]; then
  echo "RESULT: FAIL ($((n_fail + n_unknown)) gate(s) failed/unknown)."
  exit 1
fi

echo "RESULT: OK (no FAIL gates; PENDING acknowledged)."
exit 0
