#!/usr/bin/env bash
# Synthetic source/asset fixtures only: no product npm, Cargo, daemon or network.
set -euo pipefail
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
helper=${1:-$root/ops/ci/web-build.sh}
source_helper=${2:-$root/scripts/source-build.sh}
if [[ ! -f $source_helper ]]; then source_helper=$root/../../scripts/source-build.sh; fi
[[ $# -le 2 ]]
# shellcheck source=ops/ci/web-build.sh
source "$helper"
# Observations populated by the sourced helper; empty values cannot satisfy tests.
jeryu_web_scratch='' jeryu_web_mode='' jeryu_web_source=''
umask 077
scratch=$(mktemp -d -t jeryu-web-build-tests.XXXXXXXX)
identity=$(stat -c '%d:%i:%u:%g:%a' "$scratch")
cleanup() {
  local result=$? links link target unexpected
  (( result == 0 )) || { printf 'retaining failed synthetic web fixtures: %s\n' "$scratch" >&2; return "$result"; }
  jeryu_web_physical_directory "$scratch" || return 1
  [[ $(stat -c '%d:%i:%u:%g:%a' "$scratch") == "$identity" ]] || return 1
  jeryu_web_no_mounts "$scratch" || return 1
  links=$(find -P "$scratch" -xdev -type l -print) || return 1
  while IFS= read -r link; do
    [[ -n $link ]] || continue
    target=$(realpath -m -- "$link") || return 1
    [[ $target == "$scratch/"* ]] || return 1
  done <<< "$links"
  unexpected=$(find -P "$scratch" -xdev \( \( -type f ! -links 1 \) -o \( ! -type f ! -type d ! -type l \) \) -print -quit) || return 1
  [[ -z $unexpected ]] || return 1
  rm -rf --one-file-system --preserve-root=all -- "$scratch"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir "$scratch/bin" "$scratch/runtime"
export TMPDIR="$scratch/runtime" TRACE="$scratch/npm.trace"
export PATH="$scratch/bin:$PATH"
cat > "$scratch/bin/npm" <<'MOCK'
#!/bin/bash
set -euo pipefail
printf '%s\n' "$*" >> "$TRACE"
case "$*" in
  ci) exit "${NPM_CI_STATUS:-0}" ;;
  'run build')
    dist=$PWD/components/jeryu-web/apps/web/dist
    mkdir -p "$dist/assets"
    if [[ ${NPM_BUNDLE:-good} != missing_index ]]; then
      printf '<div id="root"></div><script src="/assets/app.js"></script><link href="/assets/app.css">\n' > "$dist/index.html"
    fi
    if [[ ${NPM_BUNDLE:-good} != index_only ]]; then
      printf 'synthetic JavaScript\n' > "$dist/assets/app.js"
      printf 'synthetic CSS\n' > "$dist/assets/app.css"
    fi
    [[ ${NPM_BUNDLE:-good} != empty_asset ]] || : > "$dist/assets/app.js"
    [[ ${NPM_MUTATE_SOURCE:-0} == 0 ]] || printf 'source changed by synthetic builder\n' >> README.md
    exit "${NPM_BUILD_STATUS:-0}" ;;
  *) exit 97 ;;
esac
MOCK
chmod 0700 "$scratch/bin/npm"
real_git() {
  local directory=$1
  shift
  env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 \
    /usr/bin/git -C "$directory" "$@"
}
commit_fixture() {
  real_git "$1" add -A
  real_git "$1" -c user.name=Synthetic -c user.email=fixture@jeryu.invalid \
    -c commit.gpgsign=false commit --quiet -m 'Synthetic web lifecycle fixture'
}
seed() {
  local mode=$1
  repository="$scratch/$label"
  component=$repository
  [[ $mode != monorepo ]] || component=$repository/components/jeryu-deploy
  mkdir -p "$component/crates/jeryu-api"
  real_git "$repository" -c init.defaultBranch=main init --quiet --template=
  printf 'synthetic API manifest\n' > "$component/crates/jeryu-api/Cargo.toml"
  printf 'synthetic source\n' > "$repository/README.md"
  printf '**/dist/\n' > "$repository/.gitignore"
  if [[ $mode == monorepo ]]; then
    mkdir -p "$repository/components/jeryu-web/apps/web"
    printf '{"private":true}\n' > "$repository/package.json"
    printf '{"synthetic":true}\n' > "$repository/package-lock.json"
    printf '{"name":"@jeryu/web"}\n' > "$repository/components/jeryu-web/apps/web/package.json"
  elif [[ $mode == standalone ]]; then
    mkdir -p "$repository/apps/web/dist/assets"
    printf '<script src="/assets/app.js"></script>\n' > "$repository/apps/web/dist/index.html"
    printf 'synthetic vendored JavaScript\n' > "$repository/apps/web/dist/assets/app.js"
    real_git "$repository" add -f apps/web/dist
  fi
  commit_fixture "$repository"
}
expect() {
  local expected=$1 observed=0
  shift
  "$@" > "$scratch/command.stdout" 2> "$scratch/command.stderr" || observed=$?
  [[ $observed == "$expected" ]] || { printf 'case %s: expected %s, observed %s\n' "$label" "$expected" "$observed" >&2; return 1; }
}
retained() { [[ -d $jeryu_web_scratch && ! -L $jeryu_web_scratch ]]; }
success() {
  local saved=$jeryu_web_scratch
  expect 0 jeryu_web_finish 0
  [[ ! -e $saved && ! -L $saved ]]
}
passed=0
run_case() (
  label=$1
  unset NPM_CI_STATUS NPM_BUILD_STATUS NPM_BUNDLE NPM_MUTATE_SOURCE
  unset jeryu_web_active JERYU_WEB_DIST JERYU_REQUIRE_WEB JERYU_TEST_BINARY
  : > "$TRACE"
  seed monorepo
  dist=$repository/components/jeryu-web/apps/web/dist
  case "$label" in
    normal)
      export JERYU_WEB_DIST=/synthetic-untrusted-dist JERYU_TEST_BINARY=/synthetic-untrusted-binary
      expect 0 jeryu_web_begin "$component"
      [[ $JERYU_WEB_DIST == "$dist" && $JERYU_REQUIRE_WEB == 1 && ! -v JERYU_TEST_BINARY ]]
      [[ $(<"$TRACE") == $'ci\nrun build' ]]
      success ;;
    ci_failure|build_failure|source_during_build|missing_index|index_only|empty_asset)
      case "$label" in
        ci_failure) export NPM_CI_STATUS=17 ;;
        build_failure) export NPM_BUILD_STATUS=19 ;;
        source_during_build) export NPM_MUTATE_SOURCE=1 ;;
        *) export NPM_BUNDLE=$label ;;
      esac
      expect 1 jeryu_web_begin "$component"
      expect 1 jeryu_web_finish 1
      retained ;;
    source_dirty|source_hidden)
      [[ $label != source_hidden ]] || real_git "$repository" update-index --assume-unchanged README.md
      printf 'unexpected source bytes\n' >> "$repository/README.md"
      expect 1 jeryu_web_begin "$component"
      [[ ! -s $TRACE ]] ;;
    scratch_parent_link)
      mkdir "$scratch/synthetic-tmp-target"
      ln -s "$scratch/synthetic-tmp-target" "$scratch/synthetic-tmp-link"
      export TMPDIR="$scratch/synthetic-tmp-link"
      expect 1 jeryu_web_begin "$component"
      [[ ! -s $TRACE ]] ;;
    dist_link)
      mkdir "$scratch/link-target"
      ln -s "$scratch/link-target" "$dist"
      expect 1 jeryu_web_begin "$component"
      [[ ! -s $TRACE && -L $dist ]] ;;
    preexisting_git)
      mkdir -p "$dist/.git"
      printf 'synthetic Git metadata\n' > "$dist/.git/HEAD"
      expect 1 jeryu_web_begin "$component"
      [[ ! -s $TRACE && -s $dist/.git/HEAD ]] ;;
    nested_link|preexisting_hardlink)
      mkdir -p "$dist/assets"
      printf 'synthetic preexisting artifact\n' > "$dist/assets/preexisting"
      if [[ $label == nested_link ]]; then
        ln -s "$repository/README.md" "$dist/assets/linked"
      else
        ln "$dist/assets/preexisting" "$scratch/hardlink"
      fi
      expect 1 jeryu_web_begin "$component"
      [[ ! -s $TRACE ]]
      if [[ $label == preexisting_hardlink ]]; then
        [[ ! -L $scratch/hardlink && $scratch/hardlink -ef $dist/assets/preexisting ]]
        rm -- "$scratch/hardlink"
      fi ;;
    source_after|bundle_after|bundle_extra|bundle_replaced|head_after)
      expect 0 jeryu_web_begin "$component"
      case "$label" in
        source_after) printf 'changed source\n' >> "$repository/README.md" ;;
        bundle_after) printf 'changed asset\n' >> "$dist/assets/app.js" ;;
        bundle_extra) printf 'new asset\n' > "$dist/assets/extra" ;;
        bundle_replaced)
          cat "$dist/assets/app.js" > "$dist/assets/replacement"
          mv "$dist/assets/replacement" "$dist/assets/app.js" ;;
        head_after)
          real_git "$repository" -c user.name=Synthetic -c user.email=fixture@jeryu.invalid \
            -c commit.gpgsign=false commit --quiet --allow-empty -m 'Synthetic changed head' ;;
      esac
      expect 1 jeryu_web_finish 0
      retained ;;
    cargo_failure)
      expect 0 jeryu_web_begin "$component"
      expect 23 jeryu_web_finish 23
      retained ;;
    git_partial)
      jeryu_web_git() {
        real_git "$@"
        [[ $2 != ls-tree ]] || return 7
      }
      expect 1 jeryu_web_begin "$component"
      [[ ! -s $TRACE ]] ;;
    find_partial)
      mkdir -p "$dist"
      find() { command find "$@"; return 7; }
      expect 1 jeryu_web_begin "$component"
      [[ ! -s $TRACE ]] ;;
    grep_error)
      grep() { command grep "$@"; return 2; }
      expect 1 jeryu_web_begin "$component"
      retained ;;
    missing_reference|parent_reference|query_reference|double_slash|relative_reference|no_references)
      expect 0 jeryu_web_begin "$component"
      reference=/missing.js
      case "$label" in
        parent_reference) reference=/assets/../README.md ;;
        query_reference) reference='/assets/app.js?x=1' ;;
        double_slash) reference=//assets/app.js ;;
        relative_reference) reference=assets/app.js ;;
      esac
      printf '<script src="%s"></script>\n' "$reference" > "$dist/index.html"
      [[ $label != no_references ]] || printf 'synthetic page with no references\n' > "$dist/index.html"
      expect 1 jeryu_web_references "$dist"
      expect 1 jeryu_web_finish 0
      retained ;;
    cleanup_external_link)
      expect 0 jeryu_web_begin "$component"
      ln -s "$repository/README.md" "$jeryu_web_scratch/external"
      expect 1 jeryu_web_finish 0
      retained ;;
    *) exit 98 ;;
  esac
)
for label in normal ci_failure build_failure source_during_build missing_index index_only empty_asset \
  source_dirty source_hidden scratch_parent_link dist_link preexisting_git nested_link preexisting_hardlink \
  source_after bundle_after bundle_extra bundle_replaced head_after cargo_failure \
  git_partial find_partial grep_error missing_reference parent_reference query_reference \
  double_slash relative_reference no_references cleanup_external_link; do
  run_case "$label"
  passed=$((passed+1))
done
for vendor_case in clean ignored_extra changed_blob; do
  (
    label=vendor_$vendor_case
    seed standalone
    unset jeryu_web_active
    case $vendor_case in
      ignored_extra) printf 'extra ignored asset\n' > "$repository/apps/web/dist/extra" ;;
      changed_blob) printf 'changed tracked asset\n' >> "$repository/apps/web/dist/assets/app.js" ;;
    esac
    if [[ $vendor_case == clean ]]; then
      expect 0 jeryu_web_begin "$component"
      [[ $jeryu_web_mode == committed-vendor ]]
      success
    else
      expect 1 jeryu_web_begin "$component"
    fi
  )
  passed=$((passed+1))
done
# This origin contains only the synthetic manifests above. Redirecting just
# the fixed clone URL exercises real Git checkout/provenance without network.
label=synthetic_origin
seed monorepo
source_repository=$repository
source_commit=$(real_git "$source_repository" rev-parse HEAD)
source_component_tree=$(real_git "$source_repository" rev-parse HEAD:components/jeryu-deploy)
for export_case in clean wrong_tree wrong_schema wrong_component unresolved_lock invalid_sha \
  ignored_file ignored_link source_after provenance_after internal_link; do
  (
    label=export_$export_case
    seed export
    unset jeryu_web_active
    : > "$TRACE"
    : > "$scratch/clone.trace"
    jq -n --arg source "$source_commit" --arg tree "$source_component_tree" \
      '{synthetic_test_only:true,schema_version:"jeryu.split-provenance/v1",
        component:"jeryu-deploy",source_commit:$source,original_component_tree:$tree,
        lock_regeneration_required:false,publication_qualified:false}' > "$scratch/provenance-base.json"
    mutation=.
    case $export_case in
      wrong_tree) mutation='.original_component_tree="0000000000000000000000000000000000000000"' ;;
      wrong_schema) mutation='.schema_version="invalid"' ;;
      wrong_component) mutation='.component="jeryu-core"' ;;
      unresolved_lock) mutation='.lock_regeneration_required=true' ;;
      invalid_sha) mutation='.source_commit="main"' ;;
    esac
    if [[ $export_case == ignored_file || $export_case == ignored_link ]]; then
      printf '.jeryu-source.json\n' >> "$repository/.gitignore"
    fi
    if [[ $export_case == ignored_link ]]; then
      ln -s "$scratch/provenance-base.json" "$repository/.jeryu-source.json"
    else
      jq "$mutation" "$scratch/provenance-base.json" > "$repository/.jeryu-source.json"
    fi
    commit_fixture "$repository"
    jeryu_web_git() {
      local directory=$1
      shift
      if [[ $1 == clone ]]; then
        printf 'synthetic clone reached\n' >> "$scratch/clone.trace"
        [[ $# == 6 && $2 == --no-local && $3 == --no-checkout && $4 == --quiet &&
           $5 == https://github.com/neverhuman/jeryu.git &&
           $6 == "$jeryu_web_scratch/source" ]] || return 96
        real_git "$directory" clone --no-local --no-checkout --quiet "$source_repository" "$6"
      else
        real_git "$directory" "$@"
      fi
    }
    case $export_case in
      wrong_tree|wrong_schema|wrong_component|unresolved_lock|invalid_sha|ignored_file|ignored_link)
        expect 1 jeryu_web_begin "$component"
        [[ ! -s $TRACE ]]
        if [[ $export_case == ignored_file || $export_case == ignored_link ]]; then
          [[ ! -s $scratch/clone.trace ]]
        fi ;;
      *)
        export JERYU_TEST_BINARY=/synthetic-untrusted-binary
        expect 0 jeryu_web_begin "$component"
        [[ $jeryu_web_mode == public-source-export && ! -v JERYU_TEST_BINARY &&
           $JERYU_WEB_DIST == "$jeryu_web_scratch/source/components/jeryu-web/apps/web/dist" ]]
        case $export_case in
          source_after) printf 'changed fetched source\n' >> "$jeryu_web_source/README.md" ;;
          provenance_after) printf '\n{}\n' >> "$repository/.jeryu-source.json" ;;
          internal_link) ln -s source/README.md "$jeryu_web_scratch/internal" ;;
        esac
        if [[ $export_case == source_after || $export_case == provenance_after ]]; then
          expect 1 jeryu_web_finish 0
          retained
        else
          success
        fi ;;
    esac
  )
  passed=$((passed+1))
done
# Explicit local preparation must prove the same descriptor without relying on
# inherited public-URL routing. These are tiny Git fixtures, not product source.
for local_case in clean wrong_head wrong_tree dirty hidden alias source_after; do
  (
    label=local_origin_$local_case
    seed monorepo
    source_repository=$repository
    source_commit=$(real_git "$source_repository" rev-parse HEAD)
    source_component_tree=$(real_git "$source_repository" rev-parse HEAD:components/jeryu-deploy)
    label=local_export_$local_case
    seed export
    unset jeryu_web_active
    mkdir -p "$repository/scripts"
    install -m 0600 "$source_helper" "$repository/scripts/source-build.sh"
    jq -n --arg source "$source_commit" --arg tree "$source_component_tree" '
      {schema_version:"jeryu.split-provenance/v1",component:"jeryu-deploy",
       source_commit:$source,original_component_tree:$tree,
       lock_regeneration_required:false,publication_qualified:false}' > "$repository/.jeryu-source.json"
    case $local_case in
      wrong_head) sed -i "s/$source_commit/0000000000000000000000000000000000000000/" "$repository/.jeryu-source.json" ;;
      wrong_tree) sed -i "s/$source_component_tree/0000000000000000000000000000000000000000/" "$repository/.jeryu-source.json" ;;
    esac
    commit_fixture "$repository"
    prepare=$source_repository
    case $local_case in
      dirty) printf 'changed local source\n' >> "$source_repository/README.md" ;;
      hidden) real_git "$source_repository" update-index --assume-unchanged README.md ;;
      alias) ln -s "$source_repository" "$scratch/local-source-alias"; prepare=$scratch/local-source-alias ;;
    esac
    : > "$scratch/clone.trace"
    jeryu_web_git() {
      local directory=$1
      shift
      if [[ $1 == clone ]]; then
        printf 'local clone reached\n' >> "$scratch/clone.trace"
        [[ $# == 6 && $5 == "file://$source_repository" && $6 == "$jeryu_web_scratch/source" ]] || return 96
      fi
      real_git "$directory" "$@"
    }
    case $local_case in
      wrong_head|wrong_tree|dirty|hidden|alias)
        expect 1 jeryu_web_begin "$component" --prepare-local "$prepare"
        [[ ! -s $scratch/clone.trace ]]
        ;;
      *)
        expect 0 jeryu_web_begin "$component" --prepare-local "$prepare"
        [[ $jeryu_web_mode == local-source-preparation && -s $scratch/clone.trace ]]
        if [[ $local_case == source_after ]]; then
          printf 'changed original source\n' >> "$source_repository/README.md"
          expect 1 jeryu_web_finish 0
          retained
        else
          success
        fi
        ;;
    esac
  )
  passed=$((passed+1))
done
# Exercise the actual web gate with synthetic npm/Cargo processes, including a
# misleading child PASS marker followed by a nonzero exit.
cat > "$scratch/bin/cargo" <<'CARGO'
#!/bin/bash
set -euo pipefail
[[ ! -v JERYU_TEST_BINARY && $JERYU_REQUIRE_WEB == 1 &&
   -s $JERYU_WEB_DIST/index.html ]] || exit 96
case "$*" in
  'test --locked -p jeryu-api --features web --jobs 2 browser_repo_routes_serve_the_spa_shell')
    printf 'api\n' >> "$CARGO_TRACE"
    printf 'SYNTHETIC PASS marker; fixture exit still controls the result\n'
    exit "$API_STATUS" ;;
  'test --locked -p jeryu-cli --test standalone --jobs 2')
    printf 'cli\n' >> "$CARGO_TRACE"
    printf 'SYNTHETIC PASS marker; fixture exit still controls the result\n'
    exit "$CLI_STATUS" ;;
  *) exit 97 ;;
esac
CARGO
chmod 0700 "$scratch/bin/cargo"
for dispatch_case in success api_failure cli_failure; do
  (
    label=dispatch_$dispatch_case
    seed monorepo
    mkdir -p "$component/ops/ci"
    install -m 0600 "$helper" "$component/ops/ci/web-build.sh"
    install -m 0600 "$(dirname -- "$helper")/web.sh" "$component/ops/ci/web.sh"
    printf 'JERYU_CI_JOBS=2\n' > "$component/ops/ci/common.sh"
    commit_fixture "$repository"
    export CARGO_TRACE="$scratch/cargo.trace" API_STATUS=0 CLI_STATUS=0
    export JERYU_TEST_BINARY=/synthetic-untrusted-binary
    : > "$CARGO_TRACE"
    expected=0 trace=$'api\ncli'
    case $dispatch_case in
      api_failure) API_STATUS=17; expected=17; trace=api ;;
      cli_failure) CLI_STATUS=23; expected=23 ;;
    esac
    expect "$expected" bash "$component/ops/ci/web.sh"
    [[ $(<"$CARGO_TRACE") == "$trace" ]]
    if (( expected != 0 )); then
      if grep -q '^web gate: production bundle, three CLI process tests and source/bundle readback passed$' \
          "$scratch/command.stdout"; then exit 1; fi
    fi
  )
  passed=$((passed+1))
done
cleanup
trap - EXIT
printf 'Deploy web build: %s synthetic cases passed; no product build or runtime proof executed\n' "$passed"
