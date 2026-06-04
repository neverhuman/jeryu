#!/usr/bin/env bash
# coverage.sh -- produce the coverage + mutation artifacts the jankurai coverage
# audit parses, then run that audit and assert it found no HARD findings.
#
# Single source of truth for the local `coverage` lane (and any future hosted
# `coverage` workflow job): CI and local invocations run the identical command
# sequence (local/CI parity), mirroring ops/ci/proof-evidence.sh.
#
# What it does, in order:
#   1. Install (pinned) cargo-llvm-cov + cargo-mutants if missing. If an install
#      genuinely fails in this environment we DO NOT fake green: we write a
#      skip-with-receipt under target/coverage/ and exit non-zero with code 3 so
#      the gate wrapper can render PENDING (never silent PASS).
#   2. cargo llvm-cov over the five critical engine crates -> target/llvm-cov/lcov.info
#      (line_coverage source `rust-lcov` in agent/coverage-sources.toml).
#   3. cargo-mutants SCOPED to a single critical crate with a hard --timeout
#      (mutants over the full 50-crate workspace is far too slow) ->
#      target/mutants/mutants.out/outcomes.json (mirrored to
#      target/mutants/outcomes.json; mutation source `rust-mutation`).
#   4. jankurai coverage audit over agent/coverage-sources.toml, and assert the
#      reported HARD finding count is 0.
#
# Exit codes:
#   0  -> artifacts produced, audit ran, hard==0.
#   1  -> a real failure (artifact produced but hard>0, or audit/parse error).
#   3  -> a required external tool could not be installed here; skip-with-receipt
#         was written. The gate wrapper maps this to PENDING, not FAIL.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${ROOT}"

# shellcheck source=ops/ci/ci-env.sh
source "${ROOT}/ops/ci/ci-env.sh"

# Pinned tool versions. Bump deliberately; never float.
CARGO_LLVM_COV_VERSION="${CARGO_LLVM_COV_VERSION:-0.8.7}"
CARGO_MUTANTS_VERSION="${CARGO_MUTANTS_VERSION:-25.3.1}"
JERYU_JANKURAI_VERSION="${JANKURAI_VERSION:-jankurai 1.6.10}"
JERYU_JANKURAI_BIN="${JERYU_JANKURAI_BIN:-${CARGO_HOME:-$HOME/.cargo}/bin/jankurai}"

# Critical engine crates measured for line coverage.
#
# The first group is the original engine set; the second is the workcell stack,
# added so the new regression suite is measured and the changed-line gate
# (agent/coverage-sources.toml: hard_changed_line_coverage=0.90) protects it too.
# Only DETERMINISTIC workcell crates are measured here: jeryu-sandbox-linux and
# jeryu-agentbridge are deliberately excluded because their security tests
# honestly skip when a host primitive (Landlock/cgroup delegation) is absent, so
# their line coverage is host-dependent and would make a gate flaky. They are
# protected by their own escape/refute suites, not a coverage percentage.
LLVM_COV_CRATES=(
  jeryu-ci-scheduler
  jeryu-ci-compiler
  jeryu-runner-core
  jeryu-cache-core
  jeryu-cache-policy
  jeryu-api
  jeryu-egress
  jeryu-codegraph
)

# Crates whose TOTAL src line coverage is ratcheted against a committed baseline
# (ops/ci/coverage-baseline.json): coverage may not drop below the recorded
# floor, and a green run that improves it rewrites the floor upward. This is the
# "record baseline / fail-on-drop / ratchet-up" gate for the workcell surface.
RATCHET_CRATES=(jeryu-api jeryu-egress jeryu-codegraph)
COVERAGE_BASELINE="ops/ci/coverage-baseline.tsv"
# Tolerance (fraction) below the baseline before failing — absorbs trivial
# measurement jitter without letting real coverage rot.
COVERAGE_EPSILON="${JERYU_COVERAGE_EPSILON:-0.005}"
# jeryu-api's workcell surface lives behind the `web` feature; measure it.
LLVM_COV_FEATURES="${JERYU_LLVM_COV_FEATURES:-jeryu-api/web}"

# Mutation testing is expensive, so scope it tightly to one critical crate by
# default. Override with JERYU_MUTANTS_PACKAGE / JERYU_MUTANTS_TIMEOUT.
MUTANTS_PACKAGE="${JERYU_MUTANTS_PACKAGE:-jeryu-cache-policy}"
MUTANTS_TIMEOUT="${JERYU_MUTANTS_TIMEOUT:-60}"           # per-mutant test timeout (s)
MUTANTS_BUILD_TIMEOUT="${JERYU_MUTANTS_BUILD_TIMEOUT:-300}"
# cargo-mutants jobs are heavyweight (each spawns a full cargo process, which in
# turn fans out internally), so it is capped well below JERYU_CI_JOBS to avoid
# overloading the shared host (cargo-mutants itself warns above ~8).
MUTANTS_JOBS="${JERYU_MUTANTS_JOBS:-8}"

LLVM_COV_OUT="target/llvm-cov/lcov.info"
MUTANTS_OUT_DIR="target/mutants"
MUTANTS_OUTCOMES="${MUTANTS_OUT_DIR}/mutants.out/outcomes.json"
MUTANTS_OUTCOMES_MIRROR="${MUTANTS_OUT_DIR}/outcomes.json"

RECEIPT_DIR="target/coverage"
RECEIPT="${RECEIPT_DIR}/skip-receipt.txt"

log() { printf '[coverage] %s\n' "$*"; }

# skip_with_receipt <reason...>
# Record an honest skip and exit 3 (PENDING for the gate wrapper). Never green.
skip_with_receipt() {
  mkdir -p "${RECEIPT_DIR}"
  {
    echo "coverage lane: skip-with-receipt"
    echo "when:    $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "commit:  $(git rev-parse HEAD 2>/dev/null || echo unknown)"
    echo "host:    $(uname -srm 2>/dev/null || echo unknown)"
    echo "reason:  $*"
    echo
    echo "Required pinned tools:"
    echo "  cargo-llvm-cov ${CARGO_LLVM_COV_VERSION}"
    echo "  cargo-mutants  ${CARGO_MUTANTS_VERSION}"
    echo
    echo "This lane did NOT fake green. Re-run on a host where the tools can be"
    echo "installed (network access to crates.io + the rustc llvm-tools-preview"
    echo "component) to produce target/llvm-cov/lcov.info and"
    echo "target/mutants/mutants.out/outcomes.json."
  } > "${RECEIPT}"
  log "SKIP-WITH-RECEIPT written to ${RECEIPT}"
  cat "${RECEIPT}" >&2
  exit 3
}

# ensure_cargo_subcommand <subcommand> <crate> <version>
# Install a pinned cargo subcommand if the `cargo <subcommand>` invocation is
# not already available. Returns non-zero on install failure (caller decides).
ensure_cargo_subcommand() {
  local sub="$1" crate="$2" version="$3"
  if cargo "${sub}" --version >/dev/null 2>&1; then
    log "cargo ${sub} ok: $(cargo "${sub}" --version 2>&1 | head -n1)"
    return 0
  fi
  log "installing ${crate} ${version}"
  if cargo install --locked "${crate}" --version "${version}"; then
    cargo "${sub}" --version >/dev/null 2>&1
    return $?
  fi
  return 1
}

# --- 1. Tools --------------------------------------------------------------
# cargo-llvm-cov needs the rustc llvm-tools component to instrument coverage.
if ! rustup component list --installed 2>/dev/null | grep -q 'llvm-tools'; then
  log "llvm-tools component missing; attempting to add it"
  rustup component add llvm-tools-preview >/dev/null 2>&1 \
    || rustup component add llvm-tools >/dev/null 2>&1 \
    || log "could not add llvm-tools component (cargo llvm-cov may fail)"
fi

if ! ensure_cargo_subcommand llvm-cov cargo-llvm-cov "${CARGO_LLVM_COV_VERSION}"; then
  skip_with_receipt "cargo-llvm-cov ${CARGO_LLVM_COV_VERSION} could not be installed"
fi
if ! ensure_cargo_subcommand mutants cargo-mutants "${CARGO_MUTANTS_VERSION}"; then
  skip_with_receipt "cargo-mutants ${CARGO_MUTANTS_VERSION} could not be installed"
fi

# jankurai is what consumes the artifacts; without the pinned binary there is
# no audit to run. Use the Cargo-installed path directly so ~/.local/bin cannot
# shadow a different auditor version.
if ! bash "${ROOT}/ops/ci/ensure-jankurai.sh" >/dev/null ||
  [ ! -x "${JERYU_JANKURAI_BIN}" ] ||
  ! "${JERYU_JANKURAI_BIN}" --version | grep -qx "${JERYU_JANKURAI_VERSION}"; then
  skip_with_receipt "pinned ${JERYU_JANKURAI_VERSION} unavailable"
fi

# --- 2. Line coverage (cargo-llvm-cov) -------------------------------------
mkdir -p "$(dirname "${LLVM_COV_OUT}")"
COV_PKG_ARGS=()
for c in "${LLVM_COV_CRATES[@]}"; do
  COV_PKG_ARGS+=(-p "${c}")
done
# Enable the per-package features needed to measure the gated crates (the
# jeryu-api workcell surface lives behind the `web` feature). `pkg/feature`
# syntax scopes each feature to its package so the others are unaffected.
COV_FEATURE_ARGS=()
if [ -n "${LLVM_COV_FEATURES}" ]; then
  COV_FEATURE_ARGS+=(--features "${LLVM_COV_FEATURES}")
fi

log "cargo llvm-cov over ${#LLVM_COV_CRATES[@]} crates (features: ${LLVM_COV_FEATURES:-none}) -> ${LLVM_COV_OUT}"
if ! cargo llvm-cov "${COV_PKG_ARGS[@]}" "${COV_FEATURE_ARGS[@]}" \
  --lcov --output-path "${LLVM_COV_OUT}" \
  --jobs "${JERYU_CI_JOBS}"; then
  echo "[coverage] FAIL: cargo llvm-cov did not complete" >&2
  exit 1
fi
if [ ! -s "${LLVM_COV_OUT}" ]; then
  echo "[coverage] FAIL: ${LLVM_COV_OUT} was not produced" >&2
  exit 1
fi
log "line-coverage artifact ready: ${LLVM_COV_OUT} ($(wc -l < "${LLVM_COV_OUT}") lines)"

# --- 2b. Workcell coverage ratchet -----------------------------------------
# Gate the workcell crates' src line-coverage against a committed floor
# (${COVERAGE_BASELINE}): coverage may not drop below the recorded baseline
# (minus ${COVERAGE_EPSILON} jitter), and a run with
# JERYU_COVERAGE_UPDATE_BASELINE=1 ratchets the floor UPWARD (never down). This
# is the explicit "record baseline / fail-on-drop / ratchet-up" gate; it
# complements the changed-line audit in agent/coverage-sources.toml.
log "coverage ratchet: ${RATCHET_CRATES[*]} vs ${COVERAGE_BASELINE} (eps=${COVERAGE_EPSILON})"
if ! JERYU_COVERAGE_EPSILON="${COVERAGE_EPSILON}" \
  bash ops/ci/coverage_ratchet.sh "${LLVM_COV_OUT}" "${COVERAGE_BASELINE}" "${RATCHET_CRATES[@]}"; then
  echo "[coverage] FAIL: workcell coverage ratchet gate failed" >&2
  exit 1
fi

# --- 3. Mutation testing (cargo-mutants), SCOPED ---------------------------
# Scoped to one critical crate with a per-mutant timeout: mutation testing the
# whole 50-crate workspace is far too slow for a gate. cargo-mutants exits
# non-zero when surviving mutants are found; that is a *result*, not a tool
# error, so we keep going and let the jankurai audit's hard-threshold decide.
mkdir -p "${MUTANTS_OUT_DIR}"
log "cargo mutants (scoped) package=${MUTANTS_PACKAGE} timeout=${MUTANTS_TIMEOUT}s -> ${MUTANTS_OUTCOMES}"
mutants_rc=0
cargo mutants \
  -p "${MUTANTS_PACKAGE}" \
  --output "${MUTANTS_OUT_DIR}" \
  --timeout "${MUTANTS_TIMEOUT}" \
  --build-timeout "${MUTANTS_BUILD_TIMEOUT}" \
  --jobs "${MUTANTS_JOBS}" \
  --no-times || mutants_rc=$?
log "cargo mutants exited rc=${mutants_rc} (non-zero just means surviving mutants; audit decides)"

if [ ! -s "${MUTANTS_OUTCOMES}" ]; then
  echo "[coverage] FAIL: ${MUTANTS_OUTCOMES} was not produced (mutants did not run)" >&2
  exit 1
fi
# Mirror to the alternate path the config also accepts.
cp -f "${MUTANTS_OUTCOMES}" "${MUTANTS_OUTCOMES_MIRROR}"
log "mutation artifact ready: ${MUTANTS_OUTCOMES} (+ mirror ${MUTANTS_OUTCOMES_MIRROR})"

# --- 4. jankurai coverage audit + hard==0 assertion ------------------------
mkdir -p target/jankurai/coverage
log "jankurai coverage audit . --config agent/coverage-sources.toml"
audit_out="$("${JERYU_JANKURAI_BIN}" coverage audit . \
  --config agent/coverage-sources.toml \
  --json target/jankurai/coverage/coverage-audit.json \
  --md target/jankurai/coverage/coverage-audit.md 2>&1)"
# Trim trailing whitespace the CLI pads its summary line with, then keep the
# canonical `coverage-audit ...` summary line for parsing.
audit_line="$(printf '%s\n' "${audit_out}" | sed -E 's/[[:space:]]+$//' | grep -E '^coverage-audit ' | tail -n1)"

if [ -z "${audit_line}" ]; then
  echo "[coverage] FAIL: jankurai coverage audit produced no summary line" >&2
  exit 1
fi

# Parse hard=<n> / sources=<p>/<t> from the summary line.
hard="$(printf '%s\n' "${audit_line}" | sed -nE 's/.* hard=([0-9]+).*/\1/p')"
sources="$(printf '%s\n' "${audit_line}" | sed -nE 's/.* sources=([0-9]+\/[0-9]+).*/\1/p')"
present="${sources%%/*}"
log "audit summary: ${audit_line}"
log "sources present: ${sources:-unknown}  hard findings: ${hard:-unknown}"

if [ -z "${hard}" ]; then
  echo "[coverage] FAIL: could not parse hard-finding count from audit summary" >&2
  exit 1
fi
if [ "${present:-0}" -lt 2 ]; then
  echo "[coverage] FAIL: expected 2 coverage sources present, audit saw ${sources}" >&2
  exit 1
fi
if [ "${hard}" -ne 0 ]; then
  echo "[coverage] FAIL: jankurai coverage audit reported ${hard} HARD finding(s)" >&2
  exit 1
fi

log "OK: 2/2 artifacts present, jankurai coverage audit hard=0"
exit 0
