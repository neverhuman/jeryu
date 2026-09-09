#!/usr/bin/env bash
# Bind Deploy's source manifests and actual Cargo workspace lock across CI.
jeryu_deploy_workspace_lock_snapshot() (
  set -euo pipefail
  local component=$1 git_root manifest directory path identity before after digest held
  [[ $component == /* && -d $component && ! -L $component &&
     $(realpath -e -- "$component") == "$component" ]] || {
    printf 'Deploy component root must be a physical directory\n' >&2; exit 1;
  }
  git_root=$(env -i PATH=/usr/bin:/bin GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_NOSYSTEM=1 GIT_NO_REPLACE_OBJECTS=1 /usr/bin/git \
    -C "$component" rev-parse --show-toplevel) || exit 1
  [[ $component == "$git_root" || $component == "$git_root/components/jeryu-deploy" ]] || {
    printf 'Deploy component does not match a standalone or monorepo Git root\n' >&2; exit 1;
  }
  manifest=$(cargo locate-project --workspace --message-format plain \
    --manifest-path "$component/crates/jeryu-api/Cargo.toml") || exit 1
  [[ $manifest == "$git_root/Cargo.toml" ]] || {
    printf 'Cargo workspace manifest does not match the Deploy Git root\n' >&2; exit 1;
  }
  for directory in "$component" "$git_root"; do
    [[ -d $directory && ! -L $directory &&
       $(realpath -e -- "$directory") == "$directory" ]] || exit 1
    identity=$(stat -c '%d:%i:%f:%u:%g' -- "$directory") || exit 1
    printf '%s\n%s\n' "$directory" "$identity"
  done
  for path in "$component/crates/jeryu-api/Cargo.toml" "$manifest" "$git_root/Cargo.lock"; do
    [[ -f $path && ! -L $path && $(realpath -e -- "$path") == "$path" &&
       $(stat -c '%h' -- "$path") == 1 ]] || {
      printf 'Deploy workspace input must be a physical one-link file: %s\n' "$path" >&2; exit 1;
    }
    exec {held}< "$path" || exit 1
    before=$(stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "/proc/self/fd/$held") || exit 1
    identity=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "$path") || exit 1
    [[ $before == "$identity" ]] || exit 1
    digest=$(sha256sum <&"$held") || exit 1
    digest=${digest%% *}
    [[ $digest =~ ^[0-9a-f]{64}$ ]] || exit 1
    after=$(stat -Lc '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "/proc/self/fd/$held") || exit 1
    identity=$(stat -c '%d:%i:%f:%u:%g:%h:%s:%y:%z' -- "$path") || exit 1
    [[ $before == "$after" && $before == "$identity" && ! -L $path &&
       $(realpath -e -- "$path") == "$path" ]] || {
      printf 'Deploy workspace input changed while hashing: %s\n' "$path" >&2; exit 1;
    }
    exec {held}<&-
    printf '%s\n%s\n%s\n' "$path" "$before" "$digest"
  done
)

jeryu_deploy_record_workspace_lock() {
  local component=$1 snapshot
  unset jeryu_deploy_lock_component jeryu_deploy_lock_before
  snapshot=$(jeryu_deploy_workspace_lock_snapshot "$component") || return 1
  jeryu_deploy_lock_component=$component
  jeryu_deploy_lock_before=$snapshot
}

jeryu_deploy_assert_workspace_lock_unchanged() {
  local snapshot
  [[ -n ${jeryu_deploy_lock_component:-} && -n ${jeryu_deploy_lock_before:-} ]] || {
    printf 'Deploy workspace lock was not recorded successfully\n' >&2; return 1;
  }
  snapshot=$(jeryu_deploy_workspace_lock_snapshot "$jeryu_deploy_lock_component") || return 1
  [[ $snapshot == "$jeryu_deploy_lock_before" ]] || {
    printf '[pr-ci] Cargo workspace, manifests or lock changed; refusing to discard them\n' >&2
    return 1
  }
}
