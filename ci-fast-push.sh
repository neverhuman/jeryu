#!/usr/bin/env bash
# ci-fast-push.sh — run ALL local CI as fast as possible on 40 workers; on full
# green, auto-push the current HEAD to remote `main`. Real gates only — this is
# the single command you run before/instead of any PR CI.
#
#   bash ./ci-fast-push.sh        # run everything; push to main on success
#   bash -n ./ci-fast-push.sh     # syntax-check only (does NOT run)
#
# Env:
#   JERYU_CI_JOBS=40       test workers (default 40)
#   JERYU_CI_STRICT=1      make the jankurai audit a HARD gate (default: advisory
#                          until the standard score reaches its floor)
#   JERYU_CI_NO_PUSH=1     run all gates but skip the auto-push
set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || { echo "not in a git repo"; exit 1; }

JOBS="${JERYU_CI_JOBS:-40}"
START=$(date +%s)
fail=0
declare -a RESULTS

run_step() { # run_step "name" cmd...
  local name="$1"; shift
  printf '\033[1;36m▶ %s\033[0m\n' "$name"
  if "$@"; then
    RESULTS+=("PASS  $name"); printf '\033[32m✓ %s\033[0m\n' "$name"
  else
    RESULTS+=("FAIL  $name"); printf '\033[31m✗ %s FAILED\033[0m\n' "$name"; fail=1
  fi
}

# ---- gates (run them all so you see every failure, then decide on push) ----
run_step "fmt"    cargo fmt --all -- --check
run_step "clippy" cargo clippy --workspace --all-targets --all-features -- -D warnings

if command -v cargo-nextest >/dev/null 2>&1; then
  run_step "tests (nextest, ${JOBS} workers)" \
    cargo nextest run --workspace --test-threads "$JOBS" --no-fail-fast
else
  run_step "tests (${JOBS} threads)" \
    cargo test --workspace -- --test-threads="$JOBS"
fi

[ -f scripts/zero-evidence-guard.py ] && run_step "zero-evidence" python3 scripts/zero-evidence-guard.py .
[ -f scripts/check-docs.py ]          && run_step "docs-markers"  python3 scripts/check-docs.py
[ -f scripts/ci-phases.sh ]           && run_step "phase-gates"   bash scripts/ci-phases.sh

# jankurai audit: REAL score gate. Advisory until the score clears its floor
# (set JERYU_CI_STRICT=1 to block the push on it).
if command -v jankurai >/dev/null 2>&1; then
  if [ "${JERYU_CI_STRICT:-0}" = "1" ]; then
    run_step "jankurai-audit (strict)" jankurai audit .
  else
    printf '\033[1;36m▶ jankurai-audit (advisory)\033[0m\n'
    jankurai audit . || printf '\033[33m(advisory: jankurai audit below floor; not blocking push)\033[0m\n'
  fi
fi

# ---- summary ----
DUR=$(( $(date +%s) - START ))
printf '\n\033[1m── ci-fast-push summary (%ss) ──\033[0m\n' "$DUR"
for r in "${RESULTS[@]}"; do
  case "$r" in PASS*) printf '\033[32m%s\033[0m\n' "$r";; *) printf '\033[31m%s\033[0m\n' "$r";; esac
done

if [ "$fail" -ne 0 ]; then
  printf '\033[31mCI FAILED — not pushing.\033[0m\n'; exit 1
fi
printf '\033[32mALL GATES GREEN in %ss.\033[0m\n' "$DUR"

if [ "${JERYU_CI_NO_PUSH:-0}" = "1" ]; then
  echo "JERYU_CI_NO_PUSH=1 — skipping push."; exit 0
fi

# ---- auto-push to remote main on full green ----
branch=$(git rev-parse --abbrev-ref HEAD)
printf '\033[1;36m▶ pushing %s -> remote main\033[0m\n' "$branch"
if git push github HEAD:main; then
  printf '\033[32m✓ pushed to remote main\033[0m\n'
else
  printf '\033[31m✗ push rejected (remote main advanced?) — integrate latest main and retry\033[0m\n'; exit 1
fi
