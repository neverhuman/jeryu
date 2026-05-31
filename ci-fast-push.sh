#!/usr/bin/env bash
# Affected fast lane for local pushes. Builds target/ci-fast/affected-plan.json,
# runs only mapped lanes when possible, escalates shared roots to full CI, and
# pushes HEAD to origin/main only after every local gate passes.
set -uo pipefail

cd "$(git rev-parse --show-toplevel)" || { echo "not in a git repo"; exit 1; }

JOBS="${JERYU_CI_JOBS:-40}"
NO_PUSH="${JERYU_CI_NO_PUSH:-0}"
BASE_REF="${JERYU_CI_BASE_REF:-origin/main}"
PLAN="target/ci-fast/affected-plan.json"
CHANGED_LIST="target/ci-fast/changed.lst"
START=$(date +%s)
fail=0
declare -a RESULTS

for arg in "$@"; do
  case "$arg" in
    --no-push) NO_PUSH=1 ;;
    --base=*) BASE_REF="${arg#--base=}" ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

run_step() {
  local name="$1"; shift
  printf '\033[1;36m▶ %s\033[0m\n' "$name"
  if "$@"; then
    RESULTS+=("PASS  $name"); printf '\033[32m✓ %s\033[0m\n' "$name"
  else
    RESULTS+=("FAIL  $name"); printf '\033[31m✗ %s FAILED\033[0m\n' "$name"; fail=1
  fi
}

jeryu_gate() {
  local crate="$1"; shift
  if [ "$crate" = "jeryu-repogate" ]; then
    cargo run -q --release -p "${crate}" -- "$@"
    return
  fi
  local bin="target/release/${crate}"
  if [ -x "${bin}" ]; then
    "${bin}" "$@"
  else
    cargo run -q --release -p "${crate}" -- "$@"
  fi
}

has_lane() {
  jq -e --arg lane "$1" '.lanes | index($lane) != null' "$PLAN" >/dev/null
}

is_full_ci() {
  jq -e '.full_ci == true' "$PLAN" >/dev/null
}

run_tests() {
  if command -v cargo-nextest >/dev/null 2>&1; then
    cargo nextest run "$@" --test-threads "$JOBS" --no-fail-fast
  else
    cargo test "$@" --jobs "$JOBS" -- --test-threads="$JOBS"
  fi
}

write_changed_list() {
  jq -r '.changed_files[]' "$PLAN" > "$CHANGED_LIST"
}

run_step "affected-plan" \
  jeryu_gate jeryu-repogate affected-plan --base "$BASE_REF" --out "$PLAN" --workers "$JOBS"
run_step "affected changed-list" write_changed_list

run_step "fmt" cargo fmt --all -- --check

if is_full_ci; then
  run_step "clippy workspace" \
    cargo clippy --workspace --all-targets --all-features --jobs "$JOBS" -- -D warnings
  run_step "tests workspace" run_tests --workspace
  run_step "zero-evidence" jeryu_gate jeryu-evidence .
  run_step "docs-markers" jeryu_gate jeryu-mapcheck docs
  run_step "phase-gates" bash scripts/ci-phases.sh
else
  mapfile -t PACKAGES < <(jq -r '.packages[]' "$PLAN")
  if [ "${#PACKAGES[@]}" -gt 0 ]; then
    package_flags=()
    for package in "${PACKAGES[@]}"; do
      package_flags+=("-p" "$package")
    done
    run_step "check affected Rust packages" \
      cargo check "${package_flags[@]}" --all-targets --all-features --jobs "$JOBS"
    run_step "clippy affected Rust packages" \
      cargo clippy "${package_flags[@]}" --all-targets --all-features --jobs "$JOBS" -- -D warnings
    run_step "tests affected Rust packages" run_tests "${package_flags[@]}"
  else
    RESULTS+=("PASS  rust packages (none affected)")
  fi

  if has_lane api; then
    run_step "api web feature" cargo test -p jeryu-api --features web --jobs "$JOBS"
  fi
  if has_lane tui; then
    run_step "tui captures" cargo test -p jeryu-tui --jobs "$JOBS"
  fi
  if has_lane web; then
    run_step "web typecheck" bash -lc 'cd web && npm run typecheck'
    run_step "web test" bash -lc 'cd web && npm run test'
    run_step "web build" bash -lc 'cd web && npm run build'
  fi
  if has_lane db; then
    run_step "db migration analysis" \
      jankurai migrate . --analyze --out target/jankurai/migration-report.json
  fi
fi

if command -v jankurai >/dev/null 2>&1; then
  run_step "jankurai diff audit" \
    jankurai diff-audit --base-ref "$BASE_REF" --changed-list "$CHANGED_LIST" .
  run_step "jankurai audit" jankurai audit .
else
  RESULTS+=("FAIL  jankurai audit (tool missing)")
  printf '\033[31m✗ jankurai audit FAILED (tool missing)\033[0m\n'
  fail=1
fi

DUR=$(( $(date +%s) - START ))
printf '\n\033[1m── ci-fast-push summary (%ss) ──\033[0m\n' "$DUR"
for r in "${RESULTS[@]}"; do
  case "$r" in
    PASS*) printf '\033[32m%s\033[0m\n' "$r" ;;
    *) printf '\033[31m%s\033[0m\n' "$r" ;;
  esac
done

if [ "$fail" -ne 0 ]; then
  printf '\033[31mCI FAILED — not pushing.\033[0m\n'
  exit 1
fi
printf '\033[32mALL GATES GREEN in %ss.\033[0m\n' "$DUR"

if [ "$NO_PUSH" = "1" ]; then
  echo "--no-push/JERYU_CI_NO_PUSH=1 — skipping push."
  exit 0
fi

branch=$(git rev-parse --abbrev-ref HEAD)
printf '\033[1;36m▶ pushing %s -> origin main\033[0m\n' "$branch"
if git push origin HEAD:main; then
  printf '\033[32m✓ pushed to origin main\033[0m\n'
else
  printf '\033[31m✗ push rejected — integrate latest main and retry\033[0m\n'
  exit 1
fi
