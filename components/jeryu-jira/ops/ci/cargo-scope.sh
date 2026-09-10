#!/usr/bin/env bash
# Source from this component root after ops/ci/lib.sh, under set -euo pipefail.
# Publishes member_manifest and the admitted owned_packages; does not compile.
# Caller consumes owned_packages after sourcing this admission.
# shellcheck disable=SC2034
# Cargo ownership check: the split and monorepo must select the same complete set.
for tool in cargo jq; do
  command -v "$tool" >/dev/null || { printf 'missing required tool: %s\n' "$tool" >&2; exit 1; }
done
component_root=$(pwd -P)
git_root=$(env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
  GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_OPTIONAL_LOCKS=0 \
  GIT_NO_REPLACE_OBJECTS=1 /usr/bin/git -C "$component_root" rev-parse --show-toplevel)
[[ $component_root == "$git_root" || $component_root == "$git_root/components/jeryu-jira" ]] || {
  printf 'Unexpected jeryu-jira workspace location\n' >&2; exit 1;
}
member_manifest="$component_root/crates/jeryu-jira/Cargo.toml"
[[ -f $member_manifest && ! -L $member_manifest &&
   -f $git_root/Cargo.lock && ! -L $git_root/Cargo.lock ]]
cargo_metadata=$(cargo metadata --locked --format-version 1 --no-deps --manifest-path "$member_manifest")
owned_names=$(jq -ser --arg component "$component_root" --arg workspace "$git_root" \
  --argjson expected '[["crates/jeryu-jira/Cargo.toml","jeryu-jira"]]' '
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
    else error("jeryu-jira owned package set changed") end
' <<< "$cargo_metadata")
mapfile -t owned_packages <<< "$owned_names"
