#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
lane="${repo_root}/tools/security-lane.sh"
evidence="${repo_root}/target/jankurai/security/evidence.json"
tmp_root="$(realpath -e -- "${TMPDIR:-/tmp}")"
sandbox="$(mktemp -d "${tmp_root}/jeryu-security-lane-hostiles.XXXXXX")"
fake_bin="${sandbox}/bin"
mkdir -p "$fake_bin"

cleanup() {
  case "$sandbox" in
    "${tmp_root}"/jeryu-security-lane-hostiles.*) ;;
    *) return 1 ;;
  esac
  [[ -d "$sandbox" && ! -L "$sandbox" && -O "$sandbox" ]] || return 1
  rm -rf -- "$sandbox"
}
trap cleanup EXIT INT TERM HUP

fail() {
  printf 'security hostile test failed: %s\n' "$1" >&2
  exit 1
}

write_fake_tools() {
  cat >"${fake_bin}/gitleaks" <<'SH'
#!/usr/bin/env bash
cat >/dev/null
exit "${FAKE_GITLEAKS_EXIT:-0}"
SH
  cat >"${fake_bin}/actionlint" <<'SH'
#!/usr/bin/env bash
exit "${FAKE_ACTIONLINT_EXIT:-0}"
SH
  cat >"${fake_bin}/syft" <<'SH'
#!/usr/bin/env bash
[[ "${FAKE_SYFT_EXIT:-0}" == "0" ]] || exit "${FAKE_SYFT_EXIT}"
output=""
for arg in "$@"; do
  case "$arg" in
    cyclonedx-json=*) output="${arg#cyclonedx-json=}" ;;
  esac
done
[[ -n "$output" ]] || exit 2
printf '%s\n' '{"bomFormat":"CycloneDX","specVersion":"1.5"}' >"$output"
SH
  chmod 0755 "${fake_bin}/gitleaks" "${fake_bin}/actionlint" "${fake_bin}/syft"
}

assert_failure() {
  local label="$1"
  local check_name="$2"
  shift 2
  if env PATH="${fake_bin}:/usr/bin:/bin" "$@" /usr/bin/bash "$lane" >"${sandbox}/${label}.out" 2>&1; then
    fail "$label unexpectedly returned success"
  fi
  if grep -q '^security ok' "${sandbox}/${label}.out"; then
    fail "$label printed a green conclusion"
  fi
  jq -e --arg name "$check_name" \
    'any(.checks[]; .name == $name and .status == "fail") and .conclusion == "failure"' \
    "$evidence" >/dev/null || fail "$label did not preserve a failing evidence row"
}

write_fake_tools
cd "$repo_root"
env PATH="${fake_bin}:/usr/bin:/bin" /usr/bin/bash "$lane" >"${sandbox}/baseline.out" 2>&1 ||
  fail "controlled passing scanners did not produce green evidence"
jq -e '.conclusion == "success" and all(.checks[]; .status != "fail")' "$evidence" >/dev/null ||
  fail "controlled passing evidence was not green"

rm -- "${fake_bin}/gitleaks"
assert_failure missing-gitleaks tool:gitleaks
write_fake_tools
assert_failure failing-gitleaks gitleaks-detect FAKE_GITLEAKS_EXIT=17
assert_failure failing-actionlint actionlint FAKE_ACTIONLINT_EXIT=18
assert_failure failing-syft syft-sbom FAKE_SYFT_EXIT=19

ln "${fake_bin}/actionlint" "${fake_bin}/actionlint.alias"
assert_failure hardlinked-actionlint tool:actionlint
rm -- "${fake_bin}/actionlint.alias"

env PATH="${fake_bin}:/usr/bin:/bin" /usr/bin/bash "$lane" >"${sandbox}/final.out" 2>&1 ||
  fail "security lane did not recover after hostile cases"
jq -e '.conclusion == "success"' "$evidence" >/dev/null || fail "final evidence was not green"
printf 'security lane hostiles ok\n'
