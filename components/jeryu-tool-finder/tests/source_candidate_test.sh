#!/usr/bin/env bash
# Source-boundary tests only: no installed receipt or successful qualification is fabricated.
set -euo pipefail
[[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]] || {
  printf 'candidate source tests require explicit monorepo mode\n' >&2; exit 2;
}
component_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
authority="$component_root/ops/ci/source-authority.sh"
# shellcheck source=/dev/null
source "$component_root/tests/scratch.sh"
scratch="$(mktemp -d /tmp/jeryu-finder-candidate.XXXXXX)"
jeryu_record_test_scratch "$scratch"
cleanup() {
  local status=$?
  jeryu_remove_test_scratch || status=1
  exit "$status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
trap 'exit 129' HUP

root="$scratch/monorepo"
fixture="$root/components/jeryu-tool-finder"
mkdir -p "$fixture/src" "$root/components/sibling" "$root/ops"
printf 'finder\n' >"$fixture/src/main.rs"
printf 'target/\ndossiers/\n.jankurai/\n' >"$fixture/.gitignore"
printf 'lock\n' >"$root/Cargo.lock"
printf 'shared\n' >"$root/ops/shared.sh"
printf 'sibling\n' >"$root/components/sibling/input"
printf 'target/\nnode_modules/\n.jankurai/\n' >"$root/.gitignore"
fixture_git() {
  env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_NOSYSTEM=1 /usr/bin/git -C "$root" "$@"
}
fixture_git init -q
fixture_git config user.name fixture
fixture_git config user.email fixture@example.invalid
fixture_git add .
fixture_git commit -qm 'synthetic monorepo source boundary'
JERYU_MONOREPO_EXPECTED_HEAD="$(fixture_git rev-parse HEAD)"
export repo_root="$fixture"
# shellcheck source=/dev/null
source "$authority"

fail() { printf 'candidate source test failed: %s\n' "$*" >&2; exit 1; }
reject() {
  if "$@" >"$scratch/out" 2>"$scratch/error"; then fail "accepted: $1"; fi
}
[[ "$(jeryu_candidate_source_root)" == "$root" ]] || fail 'exact component did not resolve'
jeryu_candidate_shared_source_checks "$root"
component_inputs="$(jeryu_tracked_inputs_sha256)"
shared_inputs="$(jeryu_candidate_shared_inputs_sha256 "$root")"
tree="$(fixture_git rev-parse 'HEAD^{tree}')"
scope="$(jeryu_candidate_source_scope "$JERYU_MONOREPO_EXPECTED_HEAD" "$tree" "$component_inputs")"
jq -e --arg head "$JERYU_MONOREPO_EXPECTED_HEAD" --arg tree "$tree" \
  --arg component_tree "$(fixture_git rev-parse HEAD:components/jeryu-tool-finder)" \
  --arg component_inputs "$component_inputs" --arg shared_inputs "$shared_inputs" '
  .monorepo.commit == $head and .monorepo.tree == $tree and
  .monorepo.tracked_inputs_sha256 == $shared_inputs and
  .component.path == "components/jeryu-tool-finder" and .component.tree == $component_tree and
  .component.tracked_inputs_sha256 == $component_inputs and
  .governance == {protected_main:false,handover:"pending"}
' <<<"$scope" >/dev/null

# Local layout/inventory success must never substitute for the real candidate verifier.
reject jeryu_source_snapshot
grep -Fq 'lacks its fixed monorepo verifier' "$scratch/error" || fail 'missing verifier was not required'
(
  export repo_root="$root"
  reject jeryu_candidate_source_root
)
(
  export repo_root="$root/components/sibling"
  reject jeryu_candidate_source_root
)
(
  JERYU_MONOREPO_EXPECTED_HEAD="$(printf '%040d' 0)"
  reject jeryu_candidate_source_root
)
ln -s "$root" "$scratch/root-alias"
(
  export repo_root="$scratch/root-alias/components/jeryu-tool-finder"
  reject jeryu_candidate_source_root
)
mkdir "$fixture/.git"
reject jeryu_candidate_source_root
rmdir "$fixture/.git"

# Every shared area participates, while the existing component digest retains its scope.
for path in Cargo.lock ops/shared.sh components/sibling/input; do
  printf 'changed\n' >>"$root/$path"
  [[ "$(jeryu_candidate_shared_inputs_sha256 "$root")" != "$shared_inputs" ]] ||
    fail "shared input omitted from digest: $path"
  [[ "$(jeryu_tracked_inputs_sha256)" == "$component_inputs" ]] ||
    fail 'component-relative inventory changed scope'
  reject jeryu_candidate_shared_source_checks "$root"
  fixture_git checkout -q -- "$path"
done
fixture_git update-index --assume-unchanged Cargo.lock
reject jeryu_candidate_shared_source_checks "$root"
fixture_git update-index --no-assume-unchanged Cargo.lock
ln "$root/Cargo.lock" "$scratch/shared-alias"
reject jeryu_candidate_shared_inputs_sha256 "$root"
rm -- "$scratch/shared-alias"
fixture_git update-index --refresh

mkdir -p "$root/.cargo"
printf '[build]\n' >"$root/.cargo/config.toml"
printf '.cargo/\n' >"$root/.git/info/exclude"
reject jeryu_candidate_shared_source_checks "$root"
grep -Fq 'ignored shared input' "$scratch/error" || fail 'ignored shared Cargo input was omitted'
rm -- "$root/.cargo/config.toml"
rmdir "$root/.cargo"
printf '' >"$root/.git/info/exclude"
mkdir -p "$root/node_modules/package" "$root/target/proof"
printf 'derived\n' >"$root/node_modules/package/output"
printf 'derived\n' >"$root/target/proof/output"
jeryu_candidate_shared_source_checks "$root"
mkdir -p "$fixture/.cargo"
printf '[build]\n' >"$fixture/.cargo/config.toml"
printf '.cargo/\n' >"$root/.git/info/exclude"
reject jeryu_require_physical_source
grep -Fq 'ignored input outside a derived zone' "$scratch/error" ||
  fail 'Finder component ignored policy was weakened'

# Internal dispatch consumes the supplied descriptor; authentication is tested separately.
unset JERYU_CANDIDATE_JANKURAI_DESCRIPTOR
reject jeryu_candidate_score_auditor --version
exec {printf_fd}</usr/bin/printf
export JERYU_CANDIDATE_JANKURAI_DESCRIPTOR="/proc/$BASHPID/fd/$printf_fd"
[[ "$(jeryu_candidate_score_auditor '%s\n' 'literal argument')" == 'literal argument' ]] ||
  fail 'candidate descriptor dispatch changed arguments'
exec {printf_fd}<&-
printf 'candidate source boundary tests ok\n'
