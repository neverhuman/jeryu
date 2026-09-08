#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(/usr/bin/dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
wrapper="$repo_root/tools/security-lane.sh"
security_script="$repo_root/ops/ci/security.sh"
test_root="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-web-security-contract.XXXXXX")"

cleanup() {
  case "$test_root" in
    "${TMPDIR:-/tmp}"/jeryu-web-security-contract.*)
      rm -rf -- "$test_root"
      ;;
    *)
      printf 'refusing unsafe security self-test cleanup: %s\n' "$test_root" >&2
      ;;
  esac
}
trap cleanup EXIT HUP INT TERM

wrapper_fixture="$test_root/wrapper-fixture"
missing_fixture="$test_root/missing-target-fixture"
symlink_fixture="$test_root/symlink-target-fixture"
security_fixture="$test_root/security-fixture"
mkdir -p \
  "$test_root/bash-spy" \
  "$test_root/npm-spy" \
  "$test_root/empty-path" \
  "$wrapper_fixture/tools" \
  "$wrapper_fixture/ops/ci" \
  "$missing_fixture/tools" \
  "$symlink_fixture/tools" \
  "$symlink_fixture/ops/ci" \
  "$security_fixture/ops/ci" \
  "$security_fixture/apps/web"

wrapper_log="$test_root/wrapper.log"
npm_log="$test_root/npm.log"
interpreter_sentinel="$test_root/interpreter-sentinel"
argument_sentinel="$test_root/argument-sentinel"
: >"$wrapper_log"
: >"$npm_log"

cp -- "$wrapper" "$wrapper_fixture/tools/security-lane.sh"
cp -- "$wrapper" "$missing_fixture/tools/security-lane.sh"
cp -- "$wrapper" "$symlink_fixture/tools/security-lane.sh"

cat >"$wrapper_fixture/ops/ci/security.sh" <<'SPY'
#!/usr/bin/env bash
set -eu
printf '%s:%s\n' "$#" "$0" >>"$JERYU_SECURITY_WRAPPER_SPY_LOG"
exit "${JERYU_SECURITY_WRAPPER_SPY_EXIT:-0}"
SPY
chmod 0700 "$wrapper_fixture/ops/ci/security.sh"
ln -s -- "$security_script" "$symlink_fixture/ops/ci/security.sh"

cat >"$test_root/bash-spy/bash" <<'SPY'
#!/usr/bin/env bash
set -eu
: >"$JERYU_SECURITY_INTERPRETER_SENTINEL"
exit 99
SPY
chmod 0700 "$test_root/bash-spy/bash"

run_wrapper() {
  (
    cd "$test_root"
    PATH="$test_root/bash-spy:/usr/bin:/bin" \
      JERYU_SECURITY_INTERPRETER_SENTINEL="$interpreter_sentinel" \
      JERYU_SECURITY_WRAPPER_SPY_LOG="$wrapper_log" \
      JERYU_SECURITY_WRAPPER_SPY_EXIT=23 \
      /usr/bin/bash "$wrapper_fixture/tools/security-lane.sh" "$@"
  )
}

status=0
run_wrapper || status=$?
[[ "$status" -eq 23 ]] || {
  printf 'security wrapper did not preserve child status 23: %s\n' "$status" >&2
  exit 1
}
[[ "$(wc -l <"$wrapper_log")" -eq 1 ]] || {
  printf 'security wrapper did not delegate exactly once\n' >&2
  exit 1
}
[[ "$(<"$wrapper_log")" == "0:$wrapper_fixture/ops/ci/security.sh" ]] || {
  printf 'security wrapper delegated to the wrong command\n' >&2
  exit 1
}
[[ ! -e "$interpreter_sentinel" ]] || {
  printf 'security wrapper used a PATH-selected interpreter\n' >&2
  exit 1
}

injection='$(touch '"$argument_sentinel"')'
for argument in extra --help "$injection" $'line\nbreak'; do
  before="$(wc -l <"$wrapper_log")"
  status=0
  run_wrapper "$argument" || status=$?
  [[ "$status" -eq 2 ]] || {
    printf 'security wrapper accepted a forbidden argument\n' >&2
    exit 1
  }
  [[ "$(wc -l <"$wrapper_log")" -eq "$before" ]] || {
    printf 'security wrapper delegated after rejecting an argument\n' >&2
    exit 1
  }
done
[[ ! -e "$argument_sentinel" ]] || {
  printf 'security wrapper evaluated an argument\n' >&2
  exit 1
}

status=0
(
  cd "$test_root"
  PATH="$test_root/bash-spy:/usr/bin:/bin" \
    JERYU_SECURITY_INTERPRETER_SENTINEL="$interpreter_sentinel" \
    /usr/bin/bash "$missing_fixture/tools/security-lane.sh"
) >/dev/null 2>&1 || status=$?
[[ "$status" -eq 1 ]] || {
  printf 'security wrapper did not reject a missing target\n' >&2
  exit 1
}

status=0
(
  cd "$test_root"
  PATH="$test_root/bash-spy:/usr/bin:/bin" \
    JERYU_SECURITY_INTERPRETER_SENTINEL="$interpreter_sentinel" \
    /usr/bin/bash "$symlink_fixture/tools/security-lane.sh"
) >/dev/null 2>&1 || status=$?
[[ "$status" -eq 1 ]] || {
  printf 'security wrapper did not reject a symlink target\n' >&2
  exit 1
}
[[ ! -e "$interpreter_sentinel" ]] || {
  printf 'security wrapper used a PATH-selected interpreter in a refusal path\n' >&2
  exit 1
}

cat >"$test_root/npm-spy/npm" <<'SPY'
#!/usr/bin/env bash
set -eu
printf '%s\n' "$*" >>"$JERYU_SECURITY_NPM_SPY_LOG"
printf '{"auditReportVersion":2}\n'
case "$*" in
  'audit --audit-level=high --json')
    exit "${JERYU_SECURITY_ROOT_AUDIT_EXIT:-0}"
    ;;
  'audit --prefix apps/web --audit-level=high --json')
    exit "${JERYU_SECURITY_APP_AUDIT_EXIT:-0}"
    ;;
  *)
    exit 64
    ;;
esac
SPY
chmod 0700 "$test_root/npm-spy/npm"

cp -- "$security_script" "$security_fixture/ops/ci/security.sh"
: >"$security_fixture/ops/ci/lib.sh"
printf '{}\n' >"$security_fixture/package-lock.json"
printf '{}\n' >"$security_fixture/apps/web/package-lock.json"

run_security_case() {
  local root_exit="$1"
  local app_exit="$2"
  local expected_root="$3"
  local expected_app="$4"
  local expected_aggregate="$5"
  : >"$npm_log"
  (
    cd "$security_fixture"
    PATH="$test_root/npm-spy:/usr/bin:/bin" \
      JERYU_SECURITY_NPM_SPY_LOG="$npm_log" \
      JERYU_SECURITY_ROOT_AUDIT_EXIT="$root_exit" \
      JERYU_SECURITY_APP_AUDIT_EXIT="$app_exit" \
      /usr/bin/bash ops/ci/security.sh
  ) >/dev/null

  mapfile -t npm_calls <"$npm_log"
  [[ "${#npm_calls[@]}" -eq 2 ]] || {
    printf 'security lane did not execute exactly two npm audits\n' >&2
    exit 1
  }
  [[ "${npm_calls[0]}" == 'audit --audit-level=high --json' ]] || {
    printf 'security lane root npm audit arguments drifted\n' >&2
    exit 1
  }
  [[ "${npm_calls[1]}" == 'audit --prefix apps/web --audit-level=high --json' ]] || {
    printf 'security lane app npm audit arguments drifted\n' >&2
    exit 1
  }

  node -e '
    const fs = require("fs");
    const receipt = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
    if (receipt.root_npm_audit !== process.argv[2]) process.exit(1);
    if (receipt.app_npm_audit !== process.argv[3]) process.exit(1);
    if (receipt.npm_audit !== process.argv[4]) process.exit(1);
    if (!receipt.checks.includes("npm-audit")) process.exit(1);
  ' \
    "$security_fixture/target/security/evidence.json" \
    "$expected_root" \
    "$expected_app" \
    "$expected_aggregate"
}

run_security_case 0 0 clean clean clean
run_security_case 1 0 advisories-recorded clean advisories-recorded
run_security_case 0 1 clean advisories-recorded advisories-recorded

status=0
(
  cd "$security_fixture"
  PATH="$test_root/empty-path" /usr/bin/bash ops/ci/security.sh
) >/dev/null 2>&1 || status=$?
[[ "$status" -eq 1 ]] || {
  printf 'security lane did not fail when npm was absent\n' >&2
  exit 1
}

mv -- "$security_fixture/package-lock.json" "$security_fixture/package-lock.json.held"
status=0
(
  cd "$security_fixture"
  PATH="$test_root/npm-spy:/usr/bin:/bin" /usr/bin/bash ops/ci/security.sh
) >/dev/null 2>&1 || status=$?
[[ "$status" -eq 1 ]] || {
  printf 'security lane did not fail when the root lockfile was absent\n' >&2
  exit 1
}
mv -- "$security_fixture/package-lock.json.held" "$security_fixture/package-lock.json"

mv -- "$security_fixture/apps/web/package-lock.json" \
  "$security_fixture/apps/web/package-lock.json.held"
status=0
(
  cd "$security_fixture"
  PATH="$test_root/npm-spy:/usr/bin:/bin" /usr/bin/bash ops/ci/security.sh
) >/dev/null 2>&1 || status=$?
[[ "$status" -eq 1 ]] || {
  printf 'security lane did not fail when the app lockfile was absent\n' >&2
  exit 1
}
mv -- "$security_fixture/apps/web/package-lock.json.held" \
  "$security_fixture/apps/web/package-lock.json"

printf 'security wrapper and npm audit contract ok\n'
