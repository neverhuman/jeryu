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
  if (( status != 0 )); then
    printf 'retaining failed security fixtures: %s\n' "$sandbox" >&2
    exit "$status"
  fi
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
if [[ $* == --version ]]; then
  printf '%s\n' "${FAKE_CARGO_VERSION:-cargo 1.97.1 (111111111 2026-09-01)}"
  exit "${FAKE_CARGO_VERSION_EXIT:-0}"
fi
[[ "$*" == 'metadata --locked --format-version 1 --no-deps' ]] || exit 2
[[ -z ${FAKE_CARGO_TRACE:-} ]] || printf 'metadata\n' >> "$FAKE_CARGO_TRACE"
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
printf '[toolchain]\nchannel = "1.97.1"\n' >"$repo_root/rust-toolchain.toml"
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

# A standard Rustup shim may be a symlink or a hardlink to rustup. Its
# dispatcher must never execute; only the pinned admitted Cargo runs metadata.
rustup_root="$sandbox/rustup-home"
rustup_cargo="$rustup_root/toolchains/1.97.1-x86_64-unknown-linux-gnu/bin/cargo"
mkdir -p "$(dirname -- "$rustup_cargo")"
install -m 0755 "$fake_bin/cargo" "$rustup_cargo"
cat > "$fake_bin/rustup" <<'SH'
#!/usr/bin/env bash
printf 'unexpected Rustup dispatcher execution\n' >> "$FAKE_RUSTUP_TRACE"
exit 93
SH
chmod 0755 "$fake_bin/rustup"
export RUSTUP_HOME="$rustup_root" RUSTUP_TOOLCHAIN=9.9.9-hostile-override
export FAKE_RUSTUP_TRACE="$sandbox/rustup.trace" FAKE_CARGO_TRACE="$sandbox/cargo.trace"
: > "$FAKE_RUSTUP_TRACE"
rm -- "$fake_bin/cargo"
ln -s rustup "$fake_bin/cargo"
assert_rustup_pass() {
  local label=$1
  : > "$FAKE_CARGO_TRACE"
  env PATH="$fake_bin:/usr/bin:/bin" /usr/bin/bash "$lane" > "$sandbox/$label.out" 2>&1 ||
    fail "$label did not admit the pinned actual Cargo"
  jq -e --arg path "$rustup_cargo" '
    .conclusion == "success" and
    any(.checks[]; .name == "tool:cargo" and .status == "pass" and (.detail | contains($path))) and
    any(.checks[]; .name == "cargo-toolchain" and .status == "pass") and
    any(.checks[]; .name == "cargo-metadata" and .status == "pass")
  ' "$evidence" > /dev/null || fail "$label did not bind Cargo/toolchain/metadata evidence"
  [[ $(< "$FAKE_CARGO_TRACE") == metadata && ! -s $FAKE_RUSTUP_TRACE ]] ||
    fail "$label executed a proxy or omitted actual Cargo metadata"
}
assert_rustup_refusal() {
  : > "$FAKE_CARGO_TRACE"
  assert_failure "$@"
  [[ ! -s $FAKE_CARGO_TRACE && ! -s $FAKE_RUSTUP_TRACE ]] ||
    fail "$1 executed rejected Cargo metadata or the dispatcher"
}
assert_rustup_pass symlinked-rustup-cargo
rm -- "$fake_bin/cargo"
ln "$fake_bin/rustup" "$fake_bin/cargo"
assert_rustup_pass hardlinked-rustup-cargo
assert_rustup_refusal release-proxy-still-rejected tool:cargo JAIN_RELEASE_CI=1
assert_rustup_refusal wrong-cargo-version cargo-toolchain FAKE_CARGO_VERSION='cargo 9.9.9 (111111111 2026-09-01)'
assert_rustup_refusal failed-cargo-version cargo-toolchain FAKE_CARGO_VERSION_EXIT=23
chmod 0775 "$rustup_cargo"
assert_rustup_refusal writable-pinned-cargo tool:cargo
chmod 0755 "$rustup_cargo"
ln "$rustup_cargo" "$sandbox/pinned-cargo.alias"
assert_rustup_refusal hardlinked-pinned-cargo tool:cargo
rm -- "$sandbox/pinned-cargo.alias"
mv "$rustup_cargo" "$sandbox/pinned-cargo"
assert_rustup_refusal missing-pinned-cargo tool:cargo
ln -s "$sandbox/pinned-cargo" "$rustup_cargo"
assert_rustup_refusal linked-pinned-cargo tool:cargo
rm -- "$rustup_cargo"
mv "$sandbox/pinned-cargo" "$rustup_cargo"
assert_rustup_refusal relative-rustup-home tool:cargo RUSTUP_HOME=relative
printf '[toolchain]\nchannel = "stable"\n' > "$repo_root/rust-toolchain.toml"
assert_rustup_refusal unpinned-toolchain tool:cargo
printf '[toolchain]\nchannel = "1.97.1"\nchannel = "1.97.1"\n' > "$repo_root/rust-toolchain.toml"
assert_rustup_refusal duplicate-toolchain-pin tool:cargo
printf '[toolchain]\nchannel = "1.97.1"\n' > "$repo_root/rust-toolchain.toml"
mv "$repo_root/rust-toolchain.toml" "$sandbox/toolchain.toml"
assert_rustup_refusal missing-toolchain-pin tool:cargo
ln -s "$sandbox/toolchain.toml" "$repo_root/rust-toolchain.toml"
assert_rustup_refusal linked-toolchain-pin tool:cargo
rm -- "$repo_root/rust-toolchain.toml"
mv "$sandbox/toolchain.toml" "$repo_root/rust-toolchain.toml"
assert_rustup_pass restored-pinned-cargo
# Non-Rustup links stay rejected, and direct release-tool admission is unchanged.
rm -- "$fake_bin/cargo"
ln -s "$rustup_cargo" "$fake_bin/cargo"
assert_rustup_refusal unrelated-cargo-link tool:cargo
rm -- "$fake_bin/cargo"
write_fake_tools
: > "$FAKE_CARGO_TRACE"
env PATH="$fake_bin:/usr/bin:/bin" JAIN_RELEASE_CI=1 /usr/bin/bash "$lane" > "$sandbox/release-direct.out" 2>&1 ||
  fail 'direct release Cargo was rejected'
jq -e '.conclusion == "success" and any(.checks[]; .name == "cargo-metadata" and .status == "pass")' \
  "$evidence" > /dev/null || fail 'direct release Cargo did not execute metadata'
[[ $(< "$FAKE_CARGO_TRACE") == metadata && ! -s $FAKE_RUSTUP_TRACE ]] ||
  fail 'direct release binding executed a proxy or omitted Cargo'
printf 'security Rustup Cargo binding: 17 synthetic cases passed\n'

printf 'security lane hostiles ok\n'
