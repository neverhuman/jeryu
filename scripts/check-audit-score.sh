#!/usr/bin/env bash
# Root-owned validator transport; projected from exact Git source into mirrors.
# shellcheck disable=SC1091,SC2317 # projected sources and EXIT callbacks
set -euo pipefail
[[ ( $# == 4 || ( $# == 6 && $5 == --prepare-local ) ) &&
   $1 == --owner && $3 == --component-root ]] || {
  printf 'usage: check-audit-score.sh --owner OWNER --component-root ROOT [--prepare-local SOURCE]\n' >&2
  exit 2
}
owner=$2 component=$4
case $owner in
  jeryu-core|jeryu-deploy|jeryu-jira|jeryu-intelligence|jeryu-release-ops|jeryu-tool|jeryu-web) ;;
  *) printf 'unsupported score owner\n' >&2; exit 2 ;;
esac
root=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
score_git() {
  local directory=$1
  shift
  env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_NO_REPLACE_OBJECTS=1 \
    GIT_OPTIONAL_LOCKS=0 /usr/bin/git -c core.fsmonitor=false -C "$directory" "$@"
}
# Give the unchanged source-digest helper the same closed Git transport.
git() { score_git "$PWD" "$@"; }
source "$root/scripts/source-build.sh"
score_snapshot() (
  local directory=$1 status flags
  [[ $directory == /* && -d $directory && ! -L $directory &&
     $(realpath -e -- "$directory") == "$directory" &&
     $(score_git "$directory" rev-parse --show-toplevel) == "$directory" ]] || exit 1
  status=$(score_git "$directory" status --porcelain=v1 --untracked-files=all) || exit 1
  flags=$(score_git "$directory" ls-files -v) || exit 1
  # Cargo adds this completion sentinel to its fresh Git checkout, outside the tree.
  if [[ ${2:-} == fetched-cargo && $status == '?? .cargo-ok' &&
        -f $directory/.cargo-ok && ! -L $directory/.cargo-ok && -O $directory/.cargo-ok &&
        $(stat -c '%h' -- "$directory/.cargo-ok") == 1 ]]; then
    status=''
  fi
  [[ -z $status && -n $flags ]] || exit 1
  [[ ! $flags =~ (^|$'\n')[a-zS] ]] || exit 1
  score_git "$directory" rev-parse HEAD 'HEAD^{tree}' || exit 1
  stat -c '%d:%i:%u:%g:%a' -- "$directory" || exit 1
  source_digest "$directory" || exit 1
)
[[ $component == /* && -d $component && ! -L $component &&
   $(realpath -e -- "$component") == "$component" ]] || exit 1
before=$(score_snapshot "$root") || {
  printf 'score validator requires unchanged physical committed source\n' >&2; exit 1;
}
policy="$component/agent/audit-policy.toml"
report="$component/.jankurai/repo-score.json"
if [[ $component == "$root/components/$owner" ]]; then
  [[ $# == 4 && -f $root/components/jeryu-deploy/crates/jeryu-split-tool/Cargo.toml ]] || exit 1
  status=0
  (cd "$root" && cargo run --locked --offline --quiet -p jeryu-split-tool \
    --bin jeryu-split -- audit-score-check --owner "$owner" \
    --policy "$policy" --report "$report") || status=$?
  [[ $(score_snapshot "$root") == "$before" ]] || exit 1
  exit "$status"
fi
[[ $component == "$root" ]] || exit 1
# A generated descriptor is tracked source, never caller-supplied tool authority.
[[ -f $root/.jeryu-source.json && ! -L $root/.jeryu-source.json ]] || exit 1
score_git "$root" cat-file -e HEAD:.jeryu-source.json || exit 1
selection=$(jq -ser --arg owner "$owner" '
  if length != 1 then error("one split descriptor required") else .[0] end
  | select(.schema_version=="jeryu.split-provenance/v1" and .component==$owner
    and .lock_regeneration_required==false
    and (.source_commit|type)=="string" and (.original_component_tree|type)=="string"
    and (.source_commit|test("^[0-9a-f]{40}$"))
    and (.original_component_tree|test("^[0-9a-f]{40}$")))
  | [.source_commit,.original_component_tree] | @tsv' "$root/.jeryu-source.json") || exit 1
read -r revision component_tree <<< "$selection"
toolchain=$(sed -n 's/^channel = "\([0-9][0-9.]*\)"$/\1/p' "$root/rust-toolchain.toml")
[[ $toolchain =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || exit 1
source_url=https://github.com/neverhuman/jeryu.git
mode=public-source-candidate
prepare='' prepare_before=''
if [[ $# == 6 ]]; then
  prepare=$6
  # Explicit local qualification only: never an anonymous source claim or URL override.
  [[ $prepare =~ ^/[A-Za-z0-9_./-]+$ ]] || exit 1
  prepare_before=$(score_snapshot "$prepare") || exit 1
  [[ $(score_git "$prepare" rev-parse HEAD) == "$revision" &&
     $(score_git "$prepare" rev-parse "HEAD:components/$owner") == "$component_tree" ]] || exit 1
  score_git "$prepare" show HEAD:rust-toolchain.toml | cmp - "$root/rust-toolchain.toml" || exit 1
  source_url="file://$prepare"
  mode=local-source-preparation
fi
for tool in rustup jq lsof timeout; do command -v "$tool" >/dev/null || {
  printf 'standalone score source build requires %s\n' "$tool" >&2; exit 1;
}; done
rustup_bin=$(command -v rustup)
[[ $rustup_bin == /* && -x $rustup_bin ]] || exit 1
rustup_home=$("$rustup_bin" show home)
[[ $rustup_home == /* && -d $rustup_home ]] || exit 1
source "$root/tests/scratch.sh"
umask 077
scratch=$(mktemp -d -t jeryu-score-gate.XXXXXXXX)
jeryu_record_test_scratch "$scratch" || {
  printf 'retained unadmitted score source attempt: %s\n' "$scratch" >&2; exit 1;
}
active=0
finish() {
  local result=$? probe probe_status=0
  trap - EXIT
  if [[ $active != 0 && $result == 0 ]]; then result=1; fi
  if [[ $result == 0 && $active == 0 ]]; then
    probe=$(timeout --kill-after=2s 10 lsof -t +D "$scratch" 2>&1) || probe_status=$?
    if [[ $probe_status == 1 && -z $probe ]]; then
      jeryu_remove_test_scratch || result=1
    else
      printf 'score source scratch handle closure is uncertain\n' >&2
      result=1
    fi
  fi
  if [[ $result != 0 || $active != 0 ]]; then
    printf 'retained score source attempt: %s\n' "$scratch" >&2
  fi
  exit "$result"
}
trap finish EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$scratch/home" "$scratch/cargo" "$scratch/install" "$scratch/target"
printf 'score validator source build: mode=%s commit=%s\n' "$mode" "$revision" >&2
active=1
status=0
env -i PATH=/usr/bin:/bin HOME="$scratch/home" CARGO_HOME="$scratch/cargo" \
  RUSTUP_HOME="$rustup_home" GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null \
  GIT_CONFIG_NOSYSTEM=1 GIT_NO_REPLACE_OBJECTS=1 GIT_TERMINAL_PROMPT=0 CI=true \
  "$rustup_bin" run "$toolchain" cargo install --locked --jobs 2 \
  --config net.git-fetch-with-cli=true --git "$source_url" --rev "$revision" \
  --root "$scratch/install" --target-dir "$scratch/target" \
  --bin jeryu-split --message-format=json jeryu-split-tool \
  >"$scratch/build.json" 2>"$scratch/build.stderr" || status=$?
active=0
if (( status != 0 )); then
  tail -n 40 "$scratch/build.stderr" >&2
  exit "$status"
fi
binary="$scratch/install/bin/jeryu-split"
artifact=$(jq -ers '[.[]|select(.reason=="compiler-artifact"
  and .target.name=="jeryu-split" and .executable!=null)]
  | if length==1 then .[0] | [.executable,.manifest_path] | @tsv
    else error("one built validator required") end' "$scratch/build.json")
IFS=$'\t' read -r built manifest <<< "$artifact"
[[ $manifest == "$scratch/cargo/git/checkouts/"* && -f $manifest && ! -L $manifest &&
   $(realpath -e -- "$manifest") == "$manifest" ]] || exit 1
fetched=$(score_git "$(dirname -- "$manifest")" rev-parse --show-toplevel)
[[ $fetched == "$scratch/cargo/git/checkouts/"* &&
   $manifest == "$fetched/components/jeryu-deploy/crates/jeryu-split-tool/Cargo.toml" &&
   $(score_git "$fetched" rev-parse HEAD) == "$revision" &&
   $(score_git "$fetched" rev-parse "HEAD:components/$owner") == "$component_tree" ]] || exit 1
score_snapshot "$fetched" fetched-cargo >/dev/null || exit 1
for projection in rust-toolchain.toml scripts/check-audit-score.sh scripts/source-build.sh tests/scratch.sh; do
  cmp -- "$fetched/$projection" "$root/$projection" || exit 1
done
[[ $built == "$scratch/target/"* && -f $built && ! -L $built &&
   $(realpath -e -- "$built") == "$built" &&
   -f $binary && ! -L $binary && -x $binary &&
   $(realpath -e -- "$binary") == "$binary" &&
   $(stat -c '%h' -- "$binary") == 1 ]] || exit 1
cmp -- "$built" "$binary"
digest=$(sha256sum -- "$binary")
printf 'score validator built: mode=%s commit=%s binary=%s\n' "$mode" "$revision" "$digest" >&2
active=1
status=0
"$binary" audit-score-check --owner "$owner" --policy "$policy" --report "$report" || status=$?
active=0
[[ $(score_snapshot "$root") == "$before" ]] || exit 1
if [[ -n $prepare ]]; then
  [[ $(score_snapshot "$prepare") == "$prepare_before" ]] || exit 1
fi
exit "$status"
