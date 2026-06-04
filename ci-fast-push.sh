#!/usr/bin/env bash
# Affected fast lane for local pushes. Builds target/ci-fast/affected-plan.json,
# runs only mapped lanes when possible, escalates shared roots to full CI, and
# publishes the current branch through a PR path only after every local gate
# passes. Direct pushes to origin/main require --push-main.
set -uo pipefail

cd "$(git rev-parse --show-toplevel)" || { echo "not in a git repo"; exit 1; }
source "$(pwd)/ops/ci/ci-env.sh"

JOBS="${JERYU_CI_JOBS:-40}"
RUST_TEST_MODE="${JERYU_CI_RUST_TEST_MODE:-inline}"
NO_PUSH="${JERYU_CI_NO_PUSH:-0}"
FORCE_FULL="${JERYU_CI_FULL:-0}"
PUSH_MAIN="${JERYU_CI_PUSH_MAIN:-0}"
OPEN_PR="${JERYU_CI_OPEN_PR:-1}"
PR_DRAFT="${JERYU_CI_PR_DRAFT:-1}"
PR_BASE="${JERYU_CI_PR_BASE:-main}"
PR_TITLE="${JERYU_CI_PR_TITLE:-}"
BASE_REF="${JERYU_CI_BASE_REF:-origin/main}"
PUBLISH_FILE="${JERYU_CI_PUBLISH_FILE:-target/ci-fast/publish.json}"
PLAN="target/ci-fast/affected-plan.json"
CHANGED_LIST="target/ci-fast/changed.lst"
UNTRACKED_LIST="target/ci-fast/untracked.lst"
START=$(date +%s)
fail=0
declare -a RESULTS
rm -f "$PUBLISH_FILE"

for arg in "$@"; do
  case "$arg" in
    --no-push) NO_PUSH=1 ;;
    --full) FORCE_FULL=1 ;;
    --push-main) PUSH_MAIN=1 ;;
    --no-pr) OPEN_PR=0 ;;
    --pr-base=*) PR_BASE="${arg#--pr-base=}" ;;
    --pr-title=*) PR_TITLE="${arg#--pr-title=}" ;;
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

write_publish_metadata() {
  local mode="$1" branch="$2" base="$3" pr_url="${4:-}" pr_number="${5:-}" direct_escape="${6:-false}"
  local commit; commit="$(git rev-parse HEAD)"
  mkdir -p "$(dirname "$PUBLISH_FILE")"
  jq -n \
    --arg schema "jeryu.ci-fast-push.publish/v1" \
    --arg mode "$mode" \
    --arg branch "$branch" \
    --arg base "$base" \
    --arg commit "$commit" \
    --arg pr_url "$pr_url" \
    --arg pr_number "$pr_number" \
    --arg generated_at "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
    --arg direct_escape "$direct_escape" \
    '{schema:$schema,mode:$mode,branch:$branch,base:$base,commit:$commit,generated_at:$generated_at,generated_by:"ci-fast-push.sh",direct_main_escape:($direct_escape=="true"),pr:{url:$pr_url,number:$pr_number}}' \
    > "$PUBLISH_FILE"
  echo "publication metadata: $PUBLISH_FILE"
}
jeryu_gate() {
  local crate="$1"; shift
  if [ "$crate" = "jeryu-repogate" ]; then
    cargo run -q --release -p "${crate}" -- "$@"
    return
  fi
  cargo run -q --release -p "${crate}" -- "$@"
}

has_lane() {
  jq -e --arg lane "$1" '.lanes | index($lane) != null' "$PLAN" >/dev/null
}

is_full_ci() {
  [ "$FORCE_FULL" = "1" ] && return 0
  jq -e '.full_ci == true' "$PLAN" >/dev/null
}

run_tests() {
  if command -v cargo-nextest >/dev/null 2>&1; then
    cargo nextest run "$@" --test-threads "$JOBS" --no-fail-fast
  else
    cargo test "$@" --jobs "$JOBS" -- --test-threads="$JOBS"
  fi
}

rust_tests_sharded() {
  [ "$RUST_TEST_MODE" = "sharded" ]
}

validate_rust_test_mode() {
  case "$RUST_TEST_MODE" in
    inline)
      echo "rust test mode: inline"
      ;;
    sharded)
      echo "rust test mode: sharded"
      if [ "${JERYU_CI_DOCKER}" != "0" ]; then
        echo "sharded Rust tests require JERYU_CI_DOCKER=0, got '${JERYU_CI_DOCKER}'" >&2
        return 1
      fi
      if [ "${JERYU_RUNNER_EXECUTOR}" != "native" ]; then
        echo "sharded Rust tests require JERYU_RUNNER_EXECUTOR=native, got '${JERYU_RUNNER_EXECUTOR}'" >&2
        return 1
      fi
      case "${JERYU_RUNNER_CLASS}" in
        native-rust-clean|native-rust-hot) ;;
        *)
          echo "sharded Rust tests require a native Rust runner class, got '${JERYU_RUNNER_CLASS}'" >&2
          return 1
          ;;
      esac
      ;;
    *)
      echo "JERYU_CI_RUST_TEST_MODE must be inline or sharded, got '${RUST_TEST_MODE}'" >&2
      return 1
      ;;
  esac
}

record_sharded_rust_tests() {
  local name="$1"
  RESULTS+=("PASS  ${name} (external rust-test-shards matrix)")
  printf '\033[33m↷ %s covered by external rust-test-shards matrix\033[0m\n' "$name"
}

JERYU_JANKURAI_VERSION="${JANKURAI_VERSION:-jankurai 1.6.10}"
JERYU_JANKURAI_BIN="${JERYU_JANKURAI_BIN:-${CARGO_HOME:-$HOME/.cargo}/bin/jankurai}"

run_pinned_jankurai() {
  bash ops/ci/ensure-jankurai.sh >/dev/null || return 1
  if [ ! -x "${JERYU_JANKURAI_BIN}" ]; then
    echo "pinned jankurai binary missing: ${JERYU_JANKURAI_BIN}" >&2
    return 1
  fi
  if ! "${JERYU_JANKURAI_BIN}" --version | grep -qx "${JERYU_JANKURAI_VERSION}"; then
    echo "wrong pinned jankurai version at ${JERYU_JANKURAI_BIN}: $("${JERYU_JANKURAI_BIN}" --version 2>&1 || true)" >&2
    return 1
  fi
  "${JERYU_JANKURAI_BIN}" "$@"
}

write_changed_list() {
  local plan_files
  if ! plan_files="$(jq -r '.changed_files[]?' "$PLAN")"; then
    return 1
  fi

  {
    printf '%s\n' "$plan_files"
    git diff --name-only --diff-filter=ACMRTUXB "${BASE_REF}...HEAD" 2>/dev/null || true
    git diff --name-only --diff-filter=ACMRTUXB --cached
    git diff --name-only --diff-filter=ACMRTUXB
    git ls-files --others --exclude-standard
  } | sed '/^[[:space:]]*$/d' | sort -u > "$CHANGED_LIST"
}

fail_untracked_for_remote_parity() {
  git ls-files --others --exclude-standard | sort -u > "$UNTRACKED_LIST"
  if [ ! -s "$UNTRACKED_LIST" ]; then
    return 0
  fi

  echo "untracked files are present; stage or commit them before ci-fast-push can provide GitHub-parity proof:" >&2
  sed 's/^/  /' "$UNTRACKED_LIST" >&2
  return 1
}

github_clean_profile_proof() {
  env -u RUSTC_WRAPPER -u SCCACHE_DIR -u SCCACHE_CACHE_SIZE \
    -u JERYU_RUNNER_CLASS \
    JERYU_CI_PROFILE=github JERYU_CI_USE_SCCACHE=0 bash -lc '
    set -euo pipefail
    cd "$(git rev-parse --show-toplevel)"
    source ops/ci/ci-env.sh
    test "${JERYU_CI_PROFILE}" = "github"
    test "${JERYU_RUNNER_CLASS}" = "native-rust-clean"
    test "${JERYU_RUNNER_EXECUTOR}" = "native"
    test "${JERYU_CI_DOCKER}" = "0"
    test "${RUSTC_WRAPPER:-}" = ""
    jeryu_ci_profile_summary
  '
}

run_manifest_full_lanes() {
  local lane_file="target/ci-fast/full-lanes.tsv"
  jeryu_gate jeryu-repogate ci-lanes-list --full > "$lane_file" || return 1
  while IFS=$'\t' read -r lane_id lane_command; do
    [ -n "$lane_id" ] || continue
    if [ "$lane_id" = "ci-fast" ]; then
      RESULTS+=("PASS  workflow lane ci-fast (current ci-fast-push.sh)")
      continue
    fi
    if [ "$lane_id" = "rust-shards" ]; then
      if rust_tests_sharded; then
        RESULTS+=("PASS  workflow lane rust-shards (external rust-test-shards matrix)")
      else
        RESULTS+=("PASS  workflow lane rust-shards (covered by inline tests workspace)")
      fi
      continue
    fi
    run_step "workflow lane ${lane_id}" bash -lc "$lane_command"
  done < "$lane_file"
}

open_or_report_pr() {
  local branch="$1"
  if [ "$OPEN_PR" != "1" ]; then
    echo "PR creation disabled; branch is pushed. Create a PR against ${PR_BASE} before merging."
    return 0
  fi

  if ! command -v gh >/dev/null 2>&1; then
    echo "gh is required to open the default PR path; install/authenticate gh or rerun with --no-pr after pushing." >&2
    return 1
  fi
  if ! gh auth status >/dev/null 2>&1; then
    echo "gh is not authenticated; run gh auth login or rerun with --no-pr after pushing." >&2
    return 1
  fi

  local existing_json existing_url existing_number
  existing_json="$(gh pr view "$branch" --json number,url --jq '{number:.number,url:.url}' 2>/dev/null || true)"
  if [ -n "$existing_json" ]; then
    existing_url="$(jq -r '.url // ""' <<<"$existing_json")"
    existing_number="$(jq -r '(.number // "") | tostring' <<<"$existing_json")"
    echo "PR already open: $existing_url"
    write_publish_metadata pr "$branch" "$PR_BASE" "$existing_url" "$existing_number"
    return 0
  fi

  local args=(--base "$PR_BASE" --head "$branch")
  if [ "$PR_DRAFT" = "1" ]; then
    args+=(--draft)
  fi
  if [ -n "$PR_TITLE" ]; then
    args+=(--title "$PR_TITLE" --body "Local gates passed via ci-fast-push.sh.")
  else
    args+=(--fill)
  fi
  local created_output pr_json pr_url pr_number
  if ! created_output="$(gh pr create "${args[@]}")"; then
    return 1
  fi
  printf '%s\n' "$created_output"
  pr_json="$(gh pr view "$branch" --json number,url --jq '{number:.number,url:.url}' 2>/dev/null || true)"
  pr_url="$(jq -r '.url // ""' <<<"${pr_json:-{}}")"
  pr_number="$(jq -r '(.number // "") | tostring' <<<"${pr_json:-{}}")"
  if [ -z "$pr_url" ]; then
    pr_url="$(printf '%s\n' "$created_output" | sed -nE 's#.*(https://[^[:space:]]+/pull/[0-9]+).*#\1#p' | head -1)"
  fi
  if [ -z "$pr_number" ] && [ -n "$pr_url" ]; then
    pr_number="$(printf '%s\n' "$pr_url" | sed -nE 's#.*/pull/([0-9]+).*#\1#p' | head -1)"
  fi
  [ -n "$pr_url" ] || { echo "could not resolve PR URL for $branch" >&2; return 1; }
  [ -n "$pr_number" ] || { echo "could not resolve PR number for $branch" >&2; return 1; }
  write_publish_metadata pr "$branch" "$PR_BASE" "$pr_url" "$pr_number"
}

run_step "ci profile" jeryu_ci_profile_summary
run_step "rust test mode" validate_rust_test_mode
env_args=(--build-local)
if [ "$FORCE_FULL" = "1" ]; then
  env_args+=(--release-guard)
fi
run_step "jeryu environment" bash ops/ci/verify-jeryu-env.sh "${env_args[@]}"
run_step "jankurai bootstrap" bash ops/ci/ensure-jankurai.sh
run_step "ci lane drift guard" jeryu_gate jeryu-repogate ci-lanes-check
run_step "affected-plan" \
  jeryu_gate jeryu-repogate affected-plan --base "$BASE_REF" --out "$PLAN" --workers "$JOBS"
run_step "affected changed-list" write_changed_list
run_step "untracked parity guard" fail_untracked_for_remote_parity

run_step "fmt" cargo fmt --all -- --check

if is_full_ci; then
  if [ "$FORCE_FULL" = "1" ]; then
    RESULTS+=("PASS  full mode forced")
    run_step "github clean profile proof" github_clean_profile_proof
    run_step "security toolchain" bash ops/ci/security-tools.sh
  fi
  run_step "clippy workspace" \
    cargo clippy --workspace --all-targets --all-features --jobs "$JOBS" -- -D warnings
  if rust_tests_sharded; then
    record_sharded_rust_tests "tests workspace"
  else
    run_step "tests workspace" run_tests --workspace
  fi
  run_step "zero-evidence" jeryu_gate jeryu-evidence .
  run_step "docs-markers" jeryu_gate jeryu-mapcheck docs
  run_step "phase-gates" bash scripts/ci-phases.sh
  if [ "$FORCE_FULL" = "1" ]; then
    run_manifest_full_lanes
  fi
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
    if rust_tests_sharded; then
      record_sharded_rust_tests "tests affected Rust packages"
    else
      run_step "tests affected Rust packages" run_tests "${package_flags[@]}"
    fi
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
    run_step "web typecheck" bash -lc 'cd apps/web && npm run typecheck'
    run_step "web test" bash -lc 'cd apps/web && npm run test'
    run_step "web build" bash -lc 'cd apps/web && npm run build'
  fi
  if has_lane db; then
    run_step "db migration analysis" \
      run_pinned_jankurai migrate . --analyze --out target/jankurai/migration-report.json
  fi
fi

if [ -n "${JERYU_README_PUBLISH_API_URL:-}" ]; then
  run_step "publish managed README score" bash ops/ci/publish-readme-score.sh --verify
fi
run_step "affected changed-list (post-readme)" write_changed_list
run_step "jankurai diff audit" \
  run_pinned_jankurai diff-audit --base-ref "$BASE_REF" --changed-list "$CHANGED_LIST" .
run_step "jankurai audit" run_pinned_jankurai audit .

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
if [ "$PUSH_MAIN" = "1" ]; then
  printf '\033[1;36m▶ pushing %s -> origin main (--push-main)\033[0m\n' "$branch"
  if git push origin HEAD:main; then
    printf '\033[32m✓ pushed to origin main\033[0m\n'
    write_publish_metadata direct-main "$branch" main "" "" true
  else
    printf '\033[31m✗ push rejected — integrate latest main and retry\033[0m\n'
    exit 1
  fi
  exit 0
fi

case "$branch" in
  main|master|HEAD)
    echo "direct main publishing is disabled by default; switch to a PR branch or rerun with --push-main" >&2
    exit 1
    ;;
esac

printf '\033[1;36m▶ pushing %s -> origin %s\033[0m\n' "$branch" "$branch"
if git push -u origin "$branch"; then
  printf '\033[32m✓ pushed branch %s\033[0m\n' "$branch"
else
  printf '\033[31m✗ branch push rejected — integrate latest main and retry\033[0m\n'
  exit 1
fi

printf '\033[1;36m▶ opening PR %s -> %s\033[0m\n' "$branch" "$PR_BASE"
open_or_report_pr "$branch"
