#!/usr/bin/env bash
# Per-crate src line-coverage ratchet for the workcell stack.
#
# Pure bash + awk (no Python) to match the repo's Rust-core + bash-tooling
# polyglot rule — Python is confined to python/ai-service. Reads an lcov.info,
# computes each gated crate's `crates/<crate>/src/**` line coverage (LH/LF), and
# compares it to a committed baseline (TSV lines: "<crate>\t<coverage>"):
#
#   * GATE (default): fail if any gated crate is below its baseline minus
#     JERYU_COVERAGE_EPSILON. A missing baseline file is treated as
#     "establish, don't fail" so a first run cannot false-fail.
#   * UPDATE (JERYU_COVERAGE_UPDATE_BASELINE=1): rewrite the baseline to
#     max(existing, current) per crate — the floor only ratchets UP.
#
# Usage: coverage_ratchet.sh <lcov.info> <baseline.tsv> <crate>...
# Only `crates/<crate>/src/**` lines count (test code under `tests/` is excluded
# so the floor reflects production-code coverage).
set -uo pipefail

LCOV="${1:?lcov path required}"
BASELINE="${2:?baseline path required}"
shift 2
CRATES="$*"
EPS="${JERYU_COVERAGE_EPSILON:-0.005}"
UPDATE="${JERYU_COVERAGE_UPDATE_BASELINE:-0}"

if [ ! -s "${LCOV}" ]; then
  echo "[coverage-ratchet] FAIL: lcov artifact missing or empty: ${LCOV}" >&2
  exit 1
fi

# Per-crate src coverage -> "<crate>\t<coverage>" (sorted, deterministic).
current="$(
  awk -v want="${CRATES}" '
    BEGIN { n = split(want, a, " "); for (i = 1; i <= n; i++) keep[a[i]] = 1 }
    /^SF:/ {
      cur = ""
      p = substr($0, 4)
      # cargo-llvm-cov writes ABSOLUTE SF paths (e.g.
      # /home/.../crates/<name>/src/...), so locate the "crates/" segment
      # anywhere in the path rather than anchoring at the start.
      pos = index(p, "crates/")
      if (pos > 0) {
        rest = substr(p, pos + 7)
        slash = index(rest, "/")
        if (slash > 1 && substr(rest, slash, 5) == "/src/") {
          name = substr(rest, 1, slash - 1)
          if (name in keep) cur = name
        }
      }
    }
    /^LF:/ { if (cur != "") lf[cur] += substr($0, 4) }
    /^LH:/ { if (cur != "") lh[cur] += substr($0, 4) }
    /^end_of_record$/ { cur = "" }
    END { for (c in lf) if (lf[c] > 0) printf "%s\t%.4f\n", c, lh[c] / lf[c] }
  ' "${LCOV}" | sort
)"

echo "[coverage-ratchet] measured src coverage:"
printf '%s\n' "${current}" | sed 's/^/  /'

if [ "${UPDATE}" = "1" ] || [ ! -f "${BASELINE}" ]; then
  printf '%s\n' "${current}" > "${BASELINE}.cur"
  : > "${BASELINE}.prev"
  [ -f "${BASELINE}" ] && cp "${BASELINE}" "${BASELINE}.prev"
  # Merge: floor = max(existing, current) per crate (ratchet up only).
  awk '
    FNR == NR { base[$1] = $2; next }
    { cur[$1] = $2 }
    END {
      for (c in base) if (c != "") m[c] = base[c]
      for (c in cur) if (c != "" && (!(c in m) || cur[c] + 0 > m[c] + 0)) m[c] = cur[c]
      for (c in m) printf "%s\t%s\n", c, m[c]
    }
  ' "${BASELINE}.prev" "${BASELINE}.cur" | sort > "${BASELINE}"
  rm -f "${BASELINE}.cur" "${BASELINE}.prev"
  echo "[coverage-ratchet] baseline $([ "${UPDATE}" = 1 ] && echo updated || echo established):"
  sed 's/^/  /' "${BASELINE}"
  exit 0
fi

# GATE mode: fail if any gated crate dropped below its floor.
rc=0
while IFS=$'\t' read -r crate cov; do
  [ -z "${crate}" ] && continue
  base="$(awk -F'\t' -v c="${crate}" '$1 == c { print $2 }' "${BASELINE}")"
  if [ -z "${base}" ]; then
    echo "[coverage-ratchet] (no baseline yet for ${crate} — skipping its gate)"
    continue
  fi
  if awk -v cv="${cov}" -v bv="${base}" -v e="${EPS}" 'BEGIN { exit !(cv + 0 < bv - e) }'; then
    echo "[coverage-ratchet] FAIL: ${crate} ${cov} dropped below baseline ${base} (eps ${EPS})" >&2
    rc=1
  else
    echo "[coverage-ratchet] OK: ${crate} ${cov} >= baseline ${base} (eps ${EPS})"
  fi
done <<EOF
${current}
EOF
exit ${rc}
