#!/usr/bin/env bash
# Validate LLVM LCOV line records and the committed per-crate coverage floors.
# Required checks never create missing baselines. An explicit
# JERYU_COVERAGE_UPDATE_BASELINE=1 may establish or raise floors, never lower them.
set -euo pipefail
[[ $# -ge 3 ]] || { echo 'usage: coverage_ratchet.sh LCOV BASELINE CRATE...' >&2; exit 2; }
lcov=$1 baseline=$2
shift 2
update=${JERYU_COVERAGE_UPDATE_BASELINE:-0}
epsilon=${JERYU_COVERAGE_EPSILON:-0.005}
[[ $update == 0 || $update == 1 ]] || { echo 'invalid baseline update mode' >&2; exit 1; }
[[ -f $lcov && -s $lcov && ! -L $lcov ]] || { echo 'missing regular LCOV file' >&2; exit 1; }
baseline_input=$baseline
if [[ -e $baseline || -L $baseline ]]; then
  [[ -f $baseline && ! -L $baseline && $(stat -c '%h' -- "$baseline") == 1 ]] || exit 1
else
  [[ $update == 1 ]] || { echo 'required committed coverage baseline is missing' >&2; exit 1; }
  baseline_input=/dev/null
fi
for crate in "$@"; do
  [[ $crate =~ ^[A-Za-z0-9_-]+$ ]] || { echo 'invalid requested crate' >&2; exit 1; }
done
# One parser validates both inputs before emitting a result or allowing a write.
result=$(LC_ALL=C awk -v want="$*" -v eps="$epsilon" -v update="$update" '
  function fail(message) {
    print "[coverage-ratchet] FAIL: " message > "/dev/stderr"
    bad = 1
    exit 1
  }
  function integer(value) { return value ~ /^(0|[1-9][0-9]*)$/ }
  function fraction(value) { return value ~ /^(0([.][0-9]+)?|1([.]0+)?)$/ }
  BEGIN {
    if (!fraction(eps) || eps + 0 > 0.005) fail("epsilon must be within 0..0.005")
    n = split(want, requested, " ")
    for (i = 1; i <= n; i++) {
      if (requested[i] in keep) fail("duplicate requested crate")
      keep[requested[i]] = 1
    }
  }
  FILENAME == ARGV[1] {
    if (split($0, fields, "\t") != 2 || fields[1] !~ /^[A-Za-z0-9_-]+$/ || !fraction(fields[2]))
      fail("malformed baseline row " FNR)
    if (fields[1] in base) fail("duplicate baseline crate")
    base[fields[1]] = fields[2]
    next
  }
  /^TN:/ { if (active) fail("test name inside source record"); next }
  /^SF:/ {
    if (active) fail("unterminated source record")
    path = substr($0, 4)
    if (path == "" || path ~ /(^|\/)\.\.?($|\/)/ || path ~ /\/\// || path ~ /[[:cntrl:]]/)
      fail("invalid source path")
    if (path in files) fail("duplicate source record")
    files[path] = 1
    active = 1; current = ""; lf = -1; lh = -1; lines = 0; hit = 0
    delete line_seen; delete metric_seen
    count = split(path, segments, "/")
    for (i = 1; i + 2 <= count; i++)
      if (segments[i] == "crates" && segments[i+2] == "src" && segments[i+1] in keep)
        current = segments[i+1]
    next
  }
  /^DA:/ {
    if (!active) fail("line data outside source record")
    count = split(substr($0, 4), fields, ",")
    if ((count != 2 && count != 3) || !integer(fields[1]) || fields[1] + 0 == 0 ||
        !integer(fields[2]) || (count == 3 && fields[3] == "")) fail("malformed line data")
    if (fields[1] in line_seen) fail("duplicate line data")
    line_seen[fields[1]] = 1; lines++; if (fields[2] + 0 > 0) hit++
    next
  }
  /^(LF|LH|FNF|FNH|BRF|BRH):/ {
    count = split($0, fields, ":")
    if (!active || count != 2 || !integer(fields[2]) || fields[1] in metric_seen) fail("invalid or duplicate metric")
    metric_seen[fields[1]] = 1
    if (fields[1] == "LF") lf = fields[2] + 0
    if (fields[1] == "LH") lh = fields[2] + 0
    next
  }
  /^FN:[0-9]+,(.+)$/ { if (!active) fail("function outside source record"); next }
  /^FNDA:[0-9]+,.+$/ { if (!active) fail("function hits outside source record"); next }
  /^BRDA:[0-9]+,[0-9]+,[0-9]+,(-|[0-9]+)$/ { if (!active) fail("branch outside source record"); next }
  /^end_of_record$/ {
    if (!active || lf < 0 || lh < 0 || lf != lines || lh != hit) fail("inconsistent or incomplete line totals")
    if (current != "") { total[current] += lf; hits[current] += lh }
    active = 0
    next
  }
  { fail("unrecognized or malformed LCOV record at line " FNR) }
  END {
    if (bad) exit 1
    if (active) fail("unterminated final source record")
    for (crate in keep) {
      if (!(crate in total) || total[crate] <= 0) fail("requested crate has no measured lines: " crate)
      if (!(crate in base) && update != 1) fail("requested crate has no committed baseline: " crate)
      ratio = hits[crate] / total[crate]
      measured = sprintf("%.4f", ratio)
      if (update != 1 && ratio < base[crate] - eps) fail("coverage dropped below floor for " crate)
      if (update == 1 && (!(crate in base) || measured + 0 > base[crate] + 0)) base[crate] = measured
      print "[coverage-ratchet] " crate "=" measured " floor=" base[crate] " epsilon=" eps > "/dev/stderr"
    }
    if (update == 1) for (crate in base) printf "%s\t%s\n", crate, base[crate]
  }
' "$baseline_input" "$lcov")
if [[ $update == 1 ]]; then
  parent=$(dirname -- "$baseline")
  [[ $(realpath -e -- "$parent") == "$(realpath -m -s -- "$parent")" ]] || exit 1
  scratch=$(umask 077; mktemp "$parent/.coverage-baseline.XXXXXXXX")
  # Keep a failed publication for diagnosis rather than deleting an unknown path.
  printf '%s\n' "$result" | LC_ALL=C sort >"$scratch"
  mv -T -- "$scratch" "$baseline"
  printf '[coverage-ratchet] explicitly updated baseline: %s\n' "$baseline"
fi
