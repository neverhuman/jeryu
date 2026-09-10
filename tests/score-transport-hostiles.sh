#!/usr/bin/env bash
# Small transport/custody fixtures; no real compiler, registry, auditor or install.
# shellcheck disable=SC1091,SC2317
set -euo pipefail
[[ $# -le 1 ]] || exit 2
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
adapter=${1:-$root/scripts/check-audit-score.sh}
source "$root/tests/scratch.sh"
umask 077
scratch=$(mktemp -d "${TMPDIR:-/tmp}/jeryu-score-transport.XXXXXXXX")
jeryu_record_test_scratch "$scratch"
finish() {
  local result=$?
  trap - EXIT
  if (( result == 0 )); then
    jeryu_remove_test_scratch || exit 1
  else
    printf 'score transport fixture retained: %s\n' "$scratch" >&2
  fi
  exit "$result"
}
trap finish EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
fixture_git() {
  local directory=$1; shift
  env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_NO_REPLACE_OBJECTS=1 \
    /usr/bin/git -c core.fsmonitor=false -C "$directory" "$@"
}
commit() {
  fixture_git "$1" add .
  fixture_git "$1" -c user.name=Score-fixture -c user.email=score@example.invalid \
    -c commit.gpgsign=false commit -qm Fixture
}
source_root="$scratch/source" split="$scratch/split" controls="$scratch/control"
mkdir "$source_root" "$split" "$scratch/bin" "$scratch/tmp" "$scratch/rustup" "$controls"
for directory in "$source_root" "$split"; do
  mkdir -p "$directory/scripts" "$directory/tests"
  # These three individual fixture inputs are not a copied source checkout.
  cat "$adapter" >"$directory/scripts/check-audit-score.sh"
  cat "$root/scripts/source-build.sh" >"$directory/scripts/source-build.sh"
  cat "$root/tests/scratch.sh" >"$directory/tests/scratch.sh"
  printf '[toolchain]\nchannel = "1.97.1"\n' >"$directory/rust-toolchain.toml"
  printf '/target/\n/.jankurai/\n' >"$directory/.gitignore"
  fixture_git "$directory" init -q --initial-branch=main --template=
done
mkdir -p "$source_root/components/jeryu-deploy/crates/jeryu-split-tool" \
  "$source_root/components/jeryu-jira/agent" "$source_root/components/jeryu-jira/.jankurai" \
  "$split/agent" "$split/.jankurai"
printf '[workspace]\n' >"$source_root/Cargo.toml"
printf '[package]\nname="jeryu-split-tool"\nversion="0.0.0"\n' \
  >"$source_root/components/jeryu-deploy/crates/jeryu-split-tool/Cargo.toml"
printf 'minimum_score = 85\n' >"$source_root/components/jeryu-jira/agent/audit-policy.toml"
printf '{}\n' >"$source_root/components/jeryu-jira/.jankurai/repo-score.json"
printf 'minimum_score = 85\n' >"$split/agent/audit-policy.toml"
printf '{}\n' >"$split/.jankurai/repo-score.json"
commit "$source_root"
revision=$(fixture_git "$source_root" rev-parse HEAD)
component_tree=$(fixture_git "$source_root" rev-parse HEAD:components/jeryu-jira)
jq -n --arg source "$revision" --arg tree "$component_tree" \
  '{schema_version:"jeryu.split-provenance/v1",component:"jeryu-jira",
    source_commit:$source,original_component_tree:$tree,lock_regeneration_required:false,
    publication_qualified:false}' >"$split/.jeryu-source.json"
commit "$split"
# A fake rustup is the only install process. It checks the complete transport and
# creates a tiny Git fixture plus matching executable files, never invoking Cargo.
{
  printf '#!/usr/bin/env bash\nset -euo pipefail\n'
  printf 'controls=%q\nsource_root=%q\nrevision=%q\nrustup_home=%q\n' \
    "$controls" "$source_root" "$revision" "$scratch/rustup"
  cat <<'RUSTUP'
if [[ $# == 2 && $1 == show && $2 == home ]]; then printf '%s\n' "$rustup_home"; exit 0; fi
attempt=${HOME%/home}
[[ $PATH == /usr/bin:/bin && $CARGO_HOME == "$attempt/cargo" &&
   $RUSTUP_HOME == "$rustup_home" && $GIT_CONFIG_GLOBAL == /dev/null &&
   $GIT_CONFIG_SYSTEM == /dev/null && $GIT_CONFIG_NOSYSTEM == 1 &&
   $GIT_NO_REPLACE_OBJECTS == 1 && $GIT_TERMINAL_PROMPT == 0 && $CI == true &&
   -z ${SCORE_AMBIENT_SECRET+x} && -z ${GIT_CONFIG_COUNT+x} ]] || exit 98
url=https://github.com/neverhuman/jeryu.git
if [[ $(<"$controls/mode") == local ]]; then url="file://$source_root"; fi
expected=(run 1.97.1 cargo install --locked --jobs 2 --config net.git-fetch-with-cli=true
  --git "$url" --rev "$revision" --root "$attempt/install" --target-dir "$attempt/target"
  --bin jeryu-split --message-format=json jeryu-split-tool)
[[ $# == ${#expected[@]} ]] || exit 98
index=0
for argument in "$@"; do
  [[ $argument == "${expected[$index]}" ]] || exit 98
  index=$((index + 1))
done
printf '%s\n' "$attempt" >"$controls/attempt"
printf 'install\n' >>"$controls/calls"
case $(<"$controls/install") in
  fail) exit 23 ;;
  signal) kill -TERM "$PPID"; exit 0 ;;
esac
mkdir -p "$attempt/cargo/git/checkouts" "$attempt/target/release" "$attempt/install/bin"
# Only this tiny synthetic fixture is cloned; no product source is fetched.
/usr/bin/git -c core.hooksPath=/dev/null clone --quiet --no-local "$source_root" \
  "$attempt/cargo/git/checkouts/fixture"
# Cargo's real Git checkout adds this untracked completion sentinel.
case $(<"$controls/install") in
  sentinel-link) ln -s Cargo.toml "$attempt/cargo/git/checkouts/fixture/.cargo-ok" ;;
  sentinel-hardlink) ln "$attempt/cargo/git/checkouts/fixture/Cargo.toml" \
    "$attempt/cargo/git/checkouts/fixture/.cargo-ok" ;;
  *) printf '' >"$attempt/cargo/git/checkouts/fixture/.cargo-ok" ;;
esac
if [[ $(<"$controls/install") == extra ]]; then
  printf 'unexpected\n' >"$attempt/cargo/git/checkouts/fixture/untracked"
fi
binary="$attempt/target/release/jeryu-split"
{
  printf '#!/usr/bin/env bash\nset -euo pipefail\ncontrols=%q\n' "$controls"
  printf '[[ $# == 7 && $1 == audit-score-check && $2 == --owner && $3 == jeryu-jira && $4 == --policy && $6 == --report ]] || exit 98\n'
  printf 'printf "gate\\n" >>"$controls/calls"\nexit "$(<"$controls/gate")"\n'
} >"$binary"
chmod 700 "$binary"
cat "$binary" >"$attempt/install/bin/jeryu-split"
chmod 700 "$attempt/install/bin/jeryu-split"
if [[ $(<"$controls/install") == mismatch ]]; then printf '\nchanged\n' >>"$binary"; fi
jq -nc --arg executable "$binary" \
  --arg manifest "$attempt/cargo/git/checkouts/fixture/components/jeryu-deploy/crates/jeryu-split-tool/Cargo.toml" \
  '{reason:"compiler-artifact",target:{name:"jeryu-split"},executable:$executable,manifest_path:$manifest}'
RUSTUP
} >"$scratch/bin/rustup"
{
  printf '#!/usr/bin/env bash\nset -euo pipefail\ncontrols=%q\n' "$controls"
  cat <<'LSOF'
[[ $# == 3 && $1 == -t && $2 == +D && -d $3 ]] || exit 98
case $(<"$controls/lsof") in
  empty) exit 1 ;;
  handles) printf '123\n'; exit 0 ;;
  ambiguous) printf 'incomplete scan\n' >&2; exit 1 ;;
  failure) exit 2 ;;
  *) exit 98 ;;
esac
LSOF
} >"$scratch/bin/lsof"
cat >"$scratch/bin/timeout" <<'TIMEOUT'
#!/usr/bin/env bash
set -euo pipefail
[[ $# == 6 && $1 == --kill-after=2s && $2 == 10 && $3 == lsof ]] || exit 98
shift 2
exec "$@"
TIMEOUT
{
  printf '#!/usr/bin/env bash\nset -euo pipefail\ncontrols=%q\nsource_root=%q\n' "$controls" "$source_root"
  cat <<'CARGO'
expected=(run --locked --offline --quiet -p jeryu-split-tool --bin jeryu-split --
  audit-score-check --owner jeryu-jira --policy "$source_root/components/jeryu-jira/agent/audit-policy.toml"
  --report "$source_root/components/jeryu-jira/.jankurai/repo-score.json")
[[ $PWD == "$source_root" && $# == ${#expected[@]} ]] || exit 98
index=0
for argument in "$@"; do
  [[ $argument == "${expected[$index]}" ]] || exit 98
  index=$((index + 1))
done
printf 'workspace-gate\n' >>"$controls/calls"
CARGO
} >"$scratch/bin/cargo"
chmod 700 "$scratch/bin/"*
reset_controls() {
  : >"$controls/calls"
  : >"$controls/attempt"
  printf 'public\n' >"$controls/mode"
  printf 'ok\n' >"$controls/install"
  printf '0\n' >"$controls/gate"
  printf 'empty\n' >"$controls/lsof"
}
cases=0
expect() {
  local expected=$1 directory=$2 component=$3 actual; shift 3
  if PATH="$scratch/bin:/usr/bin:/bin" TMPDIR="$scratch/tmp" SCORE_AMBIENT_SECRET=fixture \
     GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=url.bad.insteadOf GIT_CONFIG_VALUE_0=https://github.com/ \
     bash "$directory/scripts/check-audit-score.sh" --owner jeryu-jira --component-root "$component" "$@" \
       >"$scratch/stdout" 2>"$scratch/stderr"; then actual=0; else actual=$?; fi
  [[ $actual == "$expected" ]] || {
    printf 'score transport case %s expected %s got %s\n' "$cases" "$expected" "$actual" >&2
    return 1
  }
  cases=$((cases + 1))
}
reset_controls
expect 0 "$source_root" "$source_root/components/jeryu-jira"
[[ $(<"$controls/calls") == workspace-gate && ! -s $controls/attempt ]]
expect 2 "$source_root" "$source_root/components/jeryu-jira" --unexpected
expect 1 "$source_root" "$source_root/components/jeryu-deploy"
printf 'dirty\n' >>"$source_root/Cargo.toml"
expect 1 "$source_root" "$source_root/components/jeryu-jira"
fixture_git "$source_root" update-index --assume-unchanged Cargo.toml
expect 1 "$source_root" "$source_root/components/jeryu-jira"
fixture_git "$source_root" update-index --no-assume-unchanged Cargo.toml
fixture_git "$source_root" show HEAD:Cargo.toml >"$source_root/Cargo.toml"
reset_controls
expect 0 "$split" "$split"
[[ $(<"$controls/calls") == $'install\ngate' && ! -e $(<"$controls/attempt") ]]
rg -q "mode=public-source-candidate commit=$revision" "$scratch/stderr"
reset_controls
printf 'local\n' >"$controls/mode"
expect 0 "$split" "$split" --prepare-local "$source_root"
[[ $(<"$controls/calls") == $'install\ngate' && ! -e $(<"$controls/attempt") ]]
rg -q "mode=local-source-preparation commit=$revision" "$scratch/stderr"
reset_controls
expect 1 "$split" "$split" --prepare-local "$split"
[[ ! -s $controls/calls ]]
for failure in fail mismatch signal sentinel-link sentinel-hardlink extra; do
  reset_controls
  printf '%s\n' "$failure" >"$controls/install"
  expected=1
  case $failure in fail) expected=23 ;; signal) expected=143 ;; esac
  expect "$expected" "$split" "$split"
  [[ -d $(<"$controls/attempt") && $(<"$controls/calls") == install ]]
done
reset_controls
printf '7\n' >"$controls/gate"
expect 7 "$split" "$split"
[[ -d $(<"$controls/attempt") && $(<"$controls/calls") == $'install\ngate' ]]
for failure in handles ambiguous failure; do
  reset_controls
  printf '%s\n' "$failure" >"$controls/lsof"
  expect 1 "$split" "$split"
  [[ -d $(<"$controls/attempt") && $(<"$controls/calls") == $'install\ngate' ]]
done
[[ $cases == 18 ]] || { printf 'score transport count changed: %s\n' "$cases" >&2; exit 1; }
printf '%s synthetic score transport/custody cases passed; no actual source install qualified\n' "$cases"
