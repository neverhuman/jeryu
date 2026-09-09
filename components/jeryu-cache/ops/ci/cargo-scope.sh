#!/usr/bin/env bash
# Source from this component root after ops/ci/lib.sh, under set -euo pipefail.
# Publishes member_manifest and the admitted owned_packages; does not compile.
# Caller consumes owned_packages after sourcing this admission.
# shellcheck disable=SC2034
# Cargo ownership check: the split and monorepo must select the same complete set.
require_tool cargo
require_tool jq
component_root=$(pwd -P)
git_root=$(env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
  GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_OPTIONAL_LOCKS=0 \
  GIT_NO_REPLACE_OBJECTS=1 /usr/bin/git -C "$component_root" rev-parse --show-toplevel)
[[ $component_root == "$git_root" || $component_root == "$git_root/components/jeryu-cache" ]] || {
  printf 'Unexpected jeryu-cache workspace location\n' >&2; exit 1;
}
member_manifest="$component_root/crates/jeryu-cache/Cargo.toml"
[[ -f $member_manifest && ! -L $member_manifest &&
   -f $git_root/Cargo.lock && ! -L $git_root/Cargo.lock ]]
cargo_metadata=$(cargo metadata --locked --offline --format-version 1 --no-deps --manifest-path "$member_manifest")
owned_names=$(jq -ser --arg component "$component_root" --arg workspace "$git_root" \
  --argjson expected '[["crates/jeryu-cache/Cargo.toml","jeryu-cache"],["crates/jeryu-cache-adversary/Cargo.toml","jeryu-cache-adversary"],["crates/jeryu-cache-cli/Cargo.toml","jeryu-cache-cli"],["crates/jeryu-cache-core/Cargo.toml","jeryu-cache-core"],["crates/jeryu-cache-service/Cargo.toml","jeryu-cache-service"]]' '
  if length != 1 then error("expected one Cargo metadata document") else .[0] end
  | if type != "object" or .workspace_root != $workspace or
     (.workspace_members | type) != "array" or (.packages | type) != "array"
  then error("invalid Cargo workspace metadata") else . end
  | if (.workspace_members | length) == 0 or
       any(.workspace_members[]; type != "string") or
       (.workspace_members | unique | length) != (.workspace_members | length) or
       any(.packages[]; type != "object" or
         any(.id, .name, .manifest_path, .version; type != "string")) or
       ([.packages[].id] | unique | length) != (.packages | length)
    then error("invalid Cargo package membership") else . end
  | . as $metadata
  | [.workspace_members[] as $id | .packages[] | select(.id == $id)] as $members
  | [$members[] | select(.manifest_path | startswith($component + "/"))] as $owned
  | if ($members | length) == ($metadata.workspace_members | length) and
       ([$owned[] | [(.manifest_path | ltrimstr($component + "/")), .name]] | sort) == ($expected | sort) and
       ($component != $workspace or ($owned | length) == ($members | length))
    then $owned | sort_by(.name) | .[].name
    else error("jeryu-cache owned package set changed") end
' <<< "$cargo_metadata")
mapfile -t owned_packages <<< "$owned_names"
