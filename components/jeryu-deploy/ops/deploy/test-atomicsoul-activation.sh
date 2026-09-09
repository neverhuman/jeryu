#!/usr/bin/env bash
# Exercise the actual remote activation block with real Linux processes and a
# stateful systemctl fixture. Never contact SSH or the real service manager.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d "${TMPDIR:-/tmp}/jeryu-activation.XXXXXX")"
cleanup() {
  if [[ -f "$tmp/state/pid" ]]; then
    kill "$(cat "$tmp/state/pid")" 2>/dev/null || true
  fi
  jobs -pr | xargs -r kill 2>/dev/null || true
  wait 2>/dev/null || true
  rm -rf "$tmp"
}
trap cleanup EXIT
mkdir -p "$tmp/bin" "$tmp/state" "$tmp/release/bundle/atomicsoul-deploy"
cp /usr/bin/sleep "$tmp/release/bundle/jeryu"
sha256sum "$tmp/release/bundle/jeryu" | awk '{print $1 "  jeryu"}' \
  > "$tmp/release/bundle/atomicsoul-deploy/SHA256SUMS"
awk '/^# BEGIN VERIFIED SERVICE ACTIVATION$/ {copy=1; next}
     /^# END VERIFIED SERVICE ACTIVATION$/ {copy=0}
     copy' "$ROOT/ops/deploy/sign-and-push-atomicsoul.sh" > "$tmp/activation.sh"
test -s "$tmp/activation.sh"

cat > "$tmp/bin/systemctl" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$TEST_ROOT/state/calls"
[[ "$1" == --user ]]
shift
mode="$(cat "$TEST_ROOT/state/mode")"
case "$1" in
  daemon-reload) [[ "$mode" != reload-failure ]] ;;
  enable) [[ "$mode" != enable-failure && "$*" == 'enable jeryu.service' ]] ;;
  restart)
    [[ "$mode" != restart-failure && "$*" == 'restart jeryu.service' ]]
    kill "$(cat "$TEST_ROOT/state/pid")"
    if [[ "$mode" == wrong-executable ]]; then
      /usr/bin/tail -f /dev/null </dev/null >/dev/null 2>&1 &
    else
      "$TEST_ROOT/release/bundle/jeryu" 120 </dev/null >/dev/null 2>&1 &
    fi
    printf '%s\n' "$!" > "$TEST_ROOT/state/pid"
    ;;
  is-active) [[ "$mode" != inactive ]] ;;
  show)
    if [[ "$mode" == zero-pid ]]; then printf '0\n'
    elif [[ "$mode" == invalid-pid ]]; then printf '../1\n'
    elif [[ "$mode" == changed-pid && -f "$TEST_ROOT/state/shown" ]]; then printf '1\n'
    else cat "$TEST_ROOT/state/pid"; touch "$TEST_ROOT/state/shown"
    fi
    ;;
  *) exit 90 ;;
esac
SH
chmod +x "$tmp/bin/systemctl"

run_case() {
  local mode="$1" restart_flag="$2" expected="$3" rc=0 initial_pid
  if [[ -f "$tmp/state/pid" ]]; then kill "$(cat "$tmp/state/pid")" 2>/dev/null || true; fi
  wait 2>/dev/null || true
  rm -f "$tmp/state/shown"
  : > "$tmp/state/calls"
  printf '%s\n' "$mode" > "$tmp/state/mode"
  /usr/bin/tail -f /dev/null </dev/null >/dev/null 2>&1 &
  initial_pid=$!
  printf '%s\n' "$initial_pid" > "$tmp/state/pid"
  TEST_ROOT="$tmp" PATH="$tmp/bin:$PATH" restart="$restart_flag" release_dir="$tmp/release" \
    bash -euo pipefail "$tmp/activation.sh" > "$tmp/output" 2>&1 || rc=$?
  if [[ "$expected" == success ]]; then
    [[ "$rc" == 0 ]] || { cat "$tmp/output" >&2; return 1; }
    if [[ "$restart_flag" == 1 ]]; then
      [[ "$(cat "$tmp/state/pid")" != "$initial_pid" ]]
      grep -q 'verified active process pid=' "$tmp/output"
      grep -qx -- '--user restart jeryu.service' "$tmp/state/calls"
    else
      [[ "$(cat "$tmp/state/pid")" == "$initial_pid" ]]
      if grep -q -- '--user restart' "$tmp/state/calls"; then return 1; fi
      if grep -q 'verified active process' "$tmp/output"; then return 1; fi
    fi
  else
    [[ "$rc" != 0 ]] || { echo "accepted $mode" >&2; return 1; }
    if grep -q 'verified active process' "$tmp/output"; then return 1; fi
    if [[ "$mode" == reload-failure || "$mode" == enable-failure || "$mode" == restart-failure ]]; then
      [[ "$(cat "$tmp/state/pid")" == "$initial_pid" ]]
      kill -0 "$initial_pid"
    fi
  fi
  kill "$(cat "$tmp/state/pid")" 2>/dev/null || true
  wait "$initial_pid" 2>/dev/null || true
  printf 'activation case passed: %s restart=%s\n' "$mode" "$restart_flag"
}

run_case normal 1 success
run_case normal 0 success
for failure in reload-failure enable-failure restart-failure inactive zero-pid invalid-pid wrong-executable changed-pid; do
  run_case "$failure" 1 failure
done

restart=1 release_dir="$tmp/release" PATH="$tmp/empty" \
  /bin/bash -euo pipefail "$tmp/activation.sh" > "$tmp/output" 2>&1 && {
    echo 'accepted missing systemctl' >&2; exit 1;
  }
grep -q 'systemctl not available' "$tmp/output"
printf 'atomicsoul activation tests passed\n'
