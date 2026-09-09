#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source_root=$repo_root
tmp_root="$(realpath -e -- "${TMPDIR:-/tmp}")"
sandbox="$(mktemp -d "${tmp_root}/jeryu-security-lane-hostiles.XXXXXX")"
fake_bin="${sandbox}/bin"
mkdir -p "$fake_bin"

# Keep this helper in the portal tree so standalone legacy tests remain runnable.
# shellcheck source=/dev/null
source "${repo_root}/tests/scratch.sh"
jeryu_record_test_scratch "$sandbox"

cleanup() {
  local status=$?
  jeryu_remove_test_scratch || {
    printf 'retaining changed, linked, or mounted test scratch: %s\n' "$sandbox" >&2
    status=1
  }
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

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
  cat >"${fake_bin}/cargo" <<'SH'
#!/usr/bin/env bash
[[ "$*" == 'metadata --format-version 1 --no-deps' ]] || exit 2
printf '%s\n' '{"packages":[]}'
exit "${FAKE_CARGO_EXIT:-0}"
SH
  cat >"${fake_bin}/cargo-audit" <<'SH'
#!/usr/bin/env bash
[[ "$*" == 'audit --no-fetch --format json' ]] || exit 2
printf '%s\n' '{"vulnerabilities":{"found":false}}'
exit "${FAKE_CARGO_AUDIT_EXIT:-0}"
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
  chmod 0755 "${fake_bin}/gitleaks" "${fake_bin}/actionlint" "${fake_bin}/syft" \
    "${fake_bin}/cargo" "${fake_bin}/cargo-audit"
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

# A synthetic repository keeps deliberately invented scanner output away from
# the source checkout's real security evidence. Only the two entrypoint files
# under test come from source; manifests and workflow are small fixture inputs.
repo_root="$sandbox/repository"
mkdir -p "$repo_root/tools" "$repo_root/ops/ci" "$repo_root/.github/workflows"
install -m 755 "$source_root/tools/security-lane.sh" "$repo_root/tools/security-lane.sh"
install -m 755 "$source_root/ops/ci/lib.sh" "$repo_root/ops/ci/lib.sh"
printf 'fixture.1\n' >"$repo_root/VERSION"
printf '[workspace]\nmembers = []\n' >"$repo_root/Cargo.toml"
printf 'version = 4\n' >"$repo_root/Cargo.lock"
printf 'name: Fixture\non: push\njobs: {}\n' >"$repo_root/.github/workflows/ci.yml"
git -c core.hooksPath=/dev/null init --quiet --initial-branch=main "$repo_root"
git -C "$repo_root" -c core.hooksPath=/dev/null add .
lane="$repo_root/tools/security-lane.sh"
evidence="$repo_root/target/jankurai/security/evidence.json"
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
assert_failure failing-cargo cargo-metadata FAKE_CARGO_EXIT=20
assert_failure failing-cargo-audit cargo-audit-no-fetch FAKE_CARGO_AUDIT_EXIT=21
rm -- "${fake_bin}/cargo-audit"
assert_failure missing-cargo-audit tool:cargo-audit
write_fake_tools

ln "${fake_bin}/actionlint" "${fake_bin}/actionlint.alias"
assert_failure hardlinked-actionlint tool:actionlint
rm -- "${fake_bin}/actionlint.alias"

env PATH="${fake_bin}:/usr/bin:/bin" /usr/bin/bash "$lane" >"${sandbox}/final.out" 2>&1 ||
  fail "security lane did not recover after hostile cases"
jq -e '.conclusion == "success"' "$evidence" >/dev/null || fail "final evidence was not green"
printf 'security lane hostiles ok\n'
