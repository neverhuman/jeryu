#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
dispatcher="$repo_root/scripts/ci-local.sh"
tmp="$(mktemp -d /tmp/jeryu-release-ops-ci-dispatch.XXXXXX)"
cleanup() {
  local resolved
  resolved="$(realpath -e -- "$tmp" 2>/dev/null || true)"
  case "$resolved" in
    /tmp/jeryu-release-ops-ci-dispatch.??????)
      [[ "$resolved" == "$tmp" && ! -L "$resolved" ]] && rm -rf -- "$resolved"
      ;;
  esac
}
trap cleanup EXIT

mkdir -m 0700 "$tmp/bin"
cat >"$tmp/bin/just" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >>"${JERYU_DISPATCH_LOG:?}"
exit "${JERYU_DISPATCH_EXIT:-0}"
SH
chmod 0700 "$tmp/bin/just"

dispatch_log="$tmp/dispatch.log"
env -i HOME="$tmp" PATH="$tmp/bin:/usr/bin:/bin" JERYU_DISPATCH_LOG="$dispatch_log" JERYU_DISPATCH_EXIT=0 bash "$dispatcher" contract-drift
[[ "$(cat "$dispatch_log")" == "redline-consumer-test" ]] || {
  printf 'contract-drift did not route exactly to redline-consumer-test\n' >&2
  exit 1
}

rm -f -- "$dispatch_log"
if env -i HOME="$tmp" PATH="$tmp/bin:/usr/bin:/bin" JERYU_DISPATCH_LOG="$dispatch_log" JERYU_DISPATCH_EXIT=37 bash "$dispatcher" contract-drift; then
  printf 'contract-drift swallowed the delegated gate failure\n' >&2
  exit 1
else
  status=$?
fi
[[ "$status" == 37 && "$(cat "$dispatch_log")" == "redline-consumer-test" ]] || {
  printf 'contract-drift did not propagate the exact delegated status\n' >&2
  exit 1
}

rm -f -- "$dispatch_log"
if env -i HOME="$tmp" PATH="$tmp/bin:/usr/bin:/bin" JERYU_DISPATCH_LOG="$dispatch_log" JERYU_DISPATCH_EXIT=0 bash "$dispatcher" unsupported-contract-lane >"$tmp/unknown.stdout" 2>"$tmp/unknown.stderr"; then
  printf 'unknown CI lane unexpectedly succeeded\n' >&2
  exit 1
else
  status=$?
fi
[[ "$status" == 2 && ! -e "$dispatch_log" && ! -s "$tmp/unknown.stdout" ]] || {
  printf 'unknown CI lane did not fail closed before delegation\n' >&2
  exit 1
}
[[ "$(cat "$tmp/unknown.stderr")" == "unsupported CI lane: unsupported-contract-lane" ]] || {
  printf 'unknown CI lane emitted an unexpected diagnostic\n' >&2
  exit 1
}

printf 'ci-local dispatcher contract ok\n'
