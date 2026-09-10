#!/usr/bin/env bash
# Deploy-owned web build lifetime, shared by its gate and generated split CI.
# No reusable receipt or caller-selected dist grants proof authority.

jeryu_web_git() {
  local directory=$1
  shift
  env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_NO_REPLACE_OBJECTS=1 \
    GIT_OPTIONAL_LOCKS=0 /usr/bin/git -C "$directory" "$@"
}

jeryu_web_physical_directory() {
  [[ $1 == /* && -d $1 && ! -L $1 && $(realpath -e -- "$1") == "$1" ]]
}

jeryu_web_no_mounts() {
  local root=$1 mount_point count=0
  [[ -r /proc/self/mountinfo ]] || return 1
  while read -r _ _ _ _ mount_point _; do
    printf -v mount_point '%b' "$mount_point"
    [[ -n $mount_point ]] || return 1
    count=$((count + 1))
    [[ $mount_point != "$root" && $mount_point != "$root/"* ]] || return 1
  done </proc/self/mountinfo || return 1
  (( count > 0 ))
}

jeryu_web_scratch_matches() {
  jeryu_web_physical_directory "$jeryu_web_scratch" &&
    [[ $(stat -c '%d:%i:%u:%g:%a' -- "$jeryu_web_scratch") == "$jeryu_web_scratch_identity" ]]
}

# A full clean Git snapshot also reads every tracked byte, catching index flags
# that could otherwise hide a modified/missing file from git status.
jeryu_web_source_snapshot() (
  set -o pipefail
  local root=$1 head tree branch status record mode oid path observed before after digest
  jeryu_web_physical_directory "$root" || exit 1
  observed=$(jeryu_web_git "$root" rev-parse --show-toplevel) || exit 1
  [[ $observed == "$root" ]] || exit 1
  head=$(jeryu_web_git "$root" rev-parse HEAD) || exit 1
  tree=$(jeryu_web_git "$root" rev-parse 'HEAD^{tree}') || exit 1
  branch=$(jeryu_web_git "$root" branch --show-current) || exit 1
  status=$(jeryu_web_git "$root" status --porcelain=v1 --untracked-files=all) || exit 1
  [[ $head =~ ^[0-9a-f]{40}$ && $tree =~ ^[0-9a-f]{40}$ && -z $status ]] || exit 1
  printf '%s\n%s\n%s\n%s\n' "$root" "$head" "$tree" "$branch"
  stat -c '%d:%i:%u:%g:%a' -- "$root" || exit 1
  digest=$(
    jeryu_web_git "$root" ls-tree -r -z "$head" |
      while IFS= read -r -d '' record; do
        mode=${record%% *}; path=${record#*$'\t'}
        oid=${record%%$'\t'*}; oid=${oid##* }
        [[ $mode == 100644 || $mode == 100755 ]] || exit 1
        [[ -f $root/$path && ! -L $root/$path &&
           $(realpath -e -- "$root/$path") == "$root/$path" &&
           $(stat -c %h -- "$root/$path") == 1 ]] || exit 1
        before=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$root/$path") || exit 1
        observed=$(jeryu_web_git "$root" hash-object --no-filters -- "$root/$path") || exit 1
        after=$(stat -c '%d:%i:%u:%g:%a:%h:%s:%y:%z' -- "$root/$path") || exit 1
        [[ $observed == "$oid" && $before == "$after" && ! -L $root/$path ]] || exit 1
        printf '%s\0%s\0%s\0' "$path" "$before" "$observed"
      done | sha256sum
  ) || exit 1
  printf '%s\n' "${digest%% *}"
)

# Inspect the entire existing physical tree before npm/Vite can clear dist.
jeryu_web_bundle_snapshot() (
  set -o pipefail
  local dist=$1 required=$2 list file relative before after digest count=0
  jeryu_web_scratch_matches || exit 1
  jeryu_web_physical_directory "$(dirname -- "$dist")" || exit 1
  jeryu_web_no_mounts "$dist" || exit 1
  if [[ ! -e $dist && ! -L $dist ]]; then
    [[ $required == 0 ]] || exit 1
    printf 'absent\n'; exit 0
  fi
  jeryu_web_physical_directory "$dist" || exit 1
  list=$(mktemp "$jeryu_web_scratch/files.XXXXXXXX") || exit 1
  find -P "$dist" -xdev -print0 | LC_ALL=C sort -z > "$list" || exit 1
  while IFS= read -r -d '' file; do
    [[ ! -L $file && $(realpath -e -- "$file") == "$file" ]] || exit 1
    [[ -d $file || ( -f $file && $(stat -c %h -- "$file") == 1 ) ]] || exit 1
    relative=${file#"$dist"}
    # A misplaced checkout must not be emptied or embedded by the web build.
    [[ /$relative/ != *'/.git/'* ]] || exit 1
    before=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "$file") || exit 1
    printf '%s\0%s\0' "$relative" "$before"
    if [[ -f $file ]]; then
      [[ $required == 0 || -s $file ]] || exit 1
      digest=$(sha256sum -- "$file") || exit 1
      after=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "$file") || exit 1
      [[ $before == "$after" && ! -L $file ]] || exit 1
      printf '%s\0' "${digest%% *}"
      count=$((count + 1))
    fi
  done < "$list"
  [[ $required == 0 || ( -s $dist/index.html && $count -ge 2 ) ]] || exit 1
)

jeryu_web_bundle_digest() (
  set -o pipefail
  jeryu_web_bundle_snapshot "$1" "$2" | sha256sum
)

jeryu_web_references() {
  local dist=$1 matches references status=0 reference relative count=0
  jeryu_web_scratch_matches || return 1
  matches=$(mktemp "$jeryu_web_scratch/attributes.XXXXXXXX") || return 1
  references=$(mktemp "$jeryu_web_scratch/references.XXXXXXXX") || return 1
  grep -oE '(src|href)="[^"]+"' "$dist/index.html" > "$matches" || status=$?
  [[ $status == 0 || $status == 1 ]] || return 1
  sed -E 's/^[^=]+="([^"]+)"$/\1/' "$matches" | LC_ALL=C sort -u > "$references" || return 1
  while IFS= read -r reference; do
    [[ $reference =~ ^/[A-Za-z0-9._/-]+$ && $reference != *//* ]] || return 1
    relative=${reference#/}
    [[ /$relative/ != *'/../'* && /$relative/ != *'/./'* &&
       -s $dist/$relative && ! -L $dist/$relative &&
       $(realpath -e -- "$dist/$relative") == "$dist/$relative" ]] || return 1
    count=$((count + 1))
  done < "$references"
  (( count > 0 )) || return 1
  printf 'web gate: %s local index references validated\n' "$count"
}

jeryu_web_vendored_check() {
  local names observed record mode path count=0
  jeryu_web_scratch_matches || return 1
  names=$(mktemp "$jeryu_web_scratch/tracked.XXXXXXXX") || return 1
  observed=$(mktemp "$jeryu_web_scratch/physical.XXXXXXXX") || return 1
  jeryu_web_git "$jeryu_web_component" ls-tree -r -z HEAD -- apps/web/dist > "$names" || return 1
  while IFS= read -r -d '' record; do
    mode=${record%% *}; path=${record#*$'\t'}
    [[ $mode == 100644 && $path == apps/web/dist/* ]] || return 1
    count=$((count + 1))
  done < "$names"
  (( count >= 2 )) || return 1
  # Source snapshot already matches every committed byte to its Git blob.
  jeryu_web_git "$jeryu_web_component" ls-files --others --ignored --exclude-standard \
    -- apps/web/dist > "$observed" || return 1
  [[ ! -s $observed ]] || return 1
}

jeryu_web_begin() {
  local git_root provenance selection source_commit source_tree observed source_url
  [[ ( $# == 1 || ( $# == 3 && $2 == --prepare-local ) ) && ${jeryu_web_active:-0} == 0 ]] || return 1
  jeryu_web_prepare_source=${3:-} jeryu_web_prepare_before=''
  jeryu_web_component=$1
  jeryu_web_physical_directory "$jeryu_web_component" || return 1
  git_root=$(jeryu_web_git "$jeryu_web_component" rev-parse --show-toplevel) || return 1
  [[ $jeryu_web_component == "$git_root" ||
     $jeryu_web_component == "$git_root/components/jeryu-deploy" ]] || return 1
  [[ -f $jeryu_web_component/crates/jeryu-api/Cargo.toml &&
     ! -e $jeryu_web_component/package.json && ! -e $jeryu_web_component/apps/web/package.json ]] || return 1
  provenance=crates/jeryu-api/Cargo.toml
  [[ $jeryu_web_component == "$git_root" ]] || provenance=components/jeryu-deploy/$provenance
  jeryu_web_git "$git_root" cat-file -e "HEAD:$provenance" || return 1
  [[ -z $jeryu_web_prepare_source || $jeryu_web_component == "$git_root" ]] || return 1
  jeryu_web_component_git=$git_root
  jeryu_web_component_before=$(jeryu_web_source_snapshot "$git_root") || return 1
  jeryu_web_scratch=$(umask 077; mktemp -d -t jeryu-deploy-web.XXXXXXXX) || return 1
  jeryu_web_active=1 jeryu_web_prepared=0 jeryu_web_source='' jeryu_web_source_before=''
  jeryu_web_scratch_identity=$(stat -c '%d:%i:%u:%g:%a' -- "$jeryu_web_scratch") || return 1
  jeryu_web_scratch_matches || return 1
  [[ $(stat -c '%u:%g:%a' -- "$jeryu_web_scratch") == "$(id -u):$(id -g):700" ]] || return 1
  if [[ $jeryu_web_component != "$git_root" ]]; then
    jeryu_web_source=$git_root
    jeryu_web_mode=monorepo
  elif [[ -e $git_root/.jeryu-source.json || -L $git_root/.jeryu-source.json ]]; then
    provenance=$git_root/.jeryu-source.json
    # An ignored descriptor is not included in the held tracked-source boundary.
    observed=$(jeryu_web_git "$git_root" cat-file -t HEAD:.jeryu-source.json) || return 1
    [[ $observed == blob && -f $provenance && ! -L $provenance &&
       $(realpath -e -- "$provenance") == "$provenance" ]] || return 1
    selection=$(jq -ser '
      if length != 1 then error("one split provenance required") else .[0] end
      | select(.schema_version=="jeryu.split-provenance/v1" and
        .component=="jeryu-deploy" and .lock_regeneration_required==false and
        (.source_commit|type)=="string" and (.original_component_tree|type)=="string" and
        (.source_commit|test("^[0-9a-f]{40}$")) and
        (.original_component_tree|test("^[0-9a-f]{40}$")))
      | [.source_commit,.original_component_tree] | @tsv' "$provenance") || return 1
    read -r source_commit source_tree <<< "$selection"
    source_url=https://github.com/neverhuman/jeryu.git
    jeryu_web_mode=public-source-export
    if [[ -n $jeryu_web_prepare_source ]]; then
      # This helper is part of the held, clean exported source snapshot.
      # shellcheck source=scripts/source-build.sh
      source "$git_root/scripts/source-build.sh"
      jeryu_web_prepare_before=$(split_source_snapshot "$jeryu_web_prepare_source" "$source_commit" jeryu-deploy "$source_tree") || return 1
      source_url="file://$jeryu_web_prepare_source"
      jeryu_web_mode=local-source-preparation
      printf 'web transport: local-source-preparation commit=%s; public origin unproven\n' "$source_commit" >&2
    fi
    jeryu_web_git "$jeryu_web_scratch" clone --no-local --no-checkout --quiet \
      "$source_url" "$jeryu_web_scratch/source" || return 1
    jeryu_web_source=$jeryu_web_scratch/source
    jeryu_web_git "$jeryu_web_source" fetch --quiet --no-tags origin "$source_commit" || return 1
    jeryu_web_git "$jeryu_web_source" -c core.hooksPath=/dev/null checkout --quiet --detach "$source_commit" || return 1
    observed=$(jeryu_web_git "$jeryu_web_source" rev-parse HEAD) || return 1
    [[ $observed == "$source_commit" ]] || return 1
    observed=$(jeryu_web_git "$jeryu_web_source" rev-parse HEAD:components/jeryu-deploy) || return 1
    [[ $observed == "$source_tree" ]] || return 1
  else
    [[ -z $jeryu_web_prepare_source ]] || return 1
    jeryu_web_mode=committed-vendor
  fi
  if [[ -n $jeryu_web_source ]]; then
    if [[ $jeryu_web_source == "$git_root" ]]; then
      jeryu_web_source_before=$jeryu_web_component_before
    else
      jeryu_web_source_before=$(jeryu_web_source_snapshot "$jeryu_web_source") || return 1
    fi
    for provenance in package.json package-lock.json components/jeryu-web/apps/web/package.json; do
      jeryu_web_git "$jeryu_web_source" cat-file -e "HEAD:$provenance" || return 1
    done
    JERYU_WEB_DIST=$jeryu_web_source/components/jeryu-web/apps/web/dist
    jeryu_web_bundle_digest "$JERYU_WEB_DIST" 0 > "$jeryu_web_scratch/preexisting-dist.txt" || return 1
    (cd "$jeryu_web_source" && npm ci && npm run build) || return 1
    observed=$(jeryu_web_source_snapshot "$jeryu_web_source") || return 1
    [[ $observed == "$jeryu_web_source_before" ]] || return 1
  else
    JERYU_WEB_DIST=$git_root/apps/web/dist
    jeryu_web_vendored_check || return 1
  fi
  export JERYU_WEB_DIST JERYU_REQUIRE_WEB=1 || return 1
  unset JERYU_TEST_BINARY || return 1
  jeryu_web_dist=$JERYU_WEB_DIST
  jeryu_web_bundle_before=$(jeryu_web_bundle_digest "$JERYU_WEB_DIST" 1) || return 1
  jeryu_web_references "$JERYU_WEB_DIST" || return 1
  if [[ -n $jeryu_web_prepare_source ]]; then
    observed=$(split_source_snapshot "$jeryu_web_prepare_source" "$source_commit" jeryu-deploy "$source_tree") || return 1
    [[ $observed == "$jeryu_web_prepare_before" ]] || return 1
  fi
  jeryu_web_prepared=1
  printf 'web gate: %s bundle prepared from checked source\n' "$jeryu_web_mode"
}

jeryu_web_finish() {
  local status=$1 after link target links
  [[ ${jeryu_web_active:-0} == 1 ]] || return "$status"
  if (( status == 0 )); then
    [[ $jeryu_web_prepared == 1 ]] || status=1
    if [[ -n $jeryu_web_prepare_source ]]; then
      after=$(split_source_snapshot "$jeryu_web_prepare_source" "$(jeryu_web_git "$jeryu_web_source" rev-parse HEAD)") || status=1
      [[ $after == "$jeryu_web_prepare_before" ]] || status=1
    fi
    [[ $JERYU_WEB_DIST == "$jeryu_web_dist" && $JERYU_REQUIRE_WEB == 1 ]] || status=1
    after=$(jeryu_web_source_snapshot "$jeryu_web_component_git") || status=1
    [[ $after == "$jeryu_web_component_before" ]] || status=1
    if [[ -n $jeryu_web_source && $jeryu_web_source != "$jeryu_web_component_git" ]]; then
      after=$(jeryu_web_source_snapshot "$jeryu_web_source") || status=1
      [[ $after == "$jeryu_web_source_before" ]] || status=1
    elif [[ -z $jeryu_web_source ]]; then
      jeryu_web_vendored_check || status=1
    fi
    after=$(jeryu_web_bundle_digest "$JERYU_WEB_DIST" 1) || status=1
    [[ $after == "$jeryu_web_bundle_before" ]] || status=1
  fi
  if (( status != 0 )); then
    printf 'retaining failed or changed web proof scratch: %s\n' "$jeryu_web_scratch" >&2
    return "$status"
  fi
  jeryu_web_scratch_matches && jeryu_web_no_mounts "$jeryu_web_scratch" || return 1
  links=$(mktemp "$jeryu_web_scratch/links.XXXXXXXX") || return 1
  find -P "$jeryu_web_scratch" -xdev -type l -print0 > "$links" || return 1
  while IFS= read -r -d '' link; do
    target=$(realpath -m -- "$link") || return 1
    [[ $target == "$jeryu_web_scratch/"* ]] || {
      printf 'retaining web scratch with an external link: %s\n' "$jeryu_web_scratch" >&2
      return 1
    }
  done < "$links"
  jeryu_web_scratch_matches || return 1
  rm -rf --one-file-system --preserve-root=all -- "$jeryu_web_scratch" || return 1
  jeryu_web_active=0
}
