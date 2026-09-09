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
[[ $component_root == "$git_root" || $component_root == "$git_root/components/jeryu-release-ops" ]] || {
  printf 'Unexpected jeryu-release-ops workspace location\n' >&2; exit 1;
}
member_manifest="$component_root/crates/jeryu-bench/Cargo.toml"
[[ -f $member_manifest && ! -L $member_manifest &&
   -f $git_root/Cargo.lock && ! -L $git_root/Cargo.lock ]]
cargo_metadata=$(cargo metadata --locked --format-version 1 --no-deps --manifest-path "$member_manifest")
owned_names=$(jq -ser --arg component "$component_root" --arg workspace "$git_root" \
  --argjson expected '[["crates/jeryu-bench/Cargo.toml","jeryu-bench"],["crates/jeryu-compliance-export/Cargo.toml","jeryu-compliance-export"],["crates/jeryu-evidence/Cargo.toml","jeryu-evidence"],["crates/jeryu-git-guard/Cargo.toml","jeryu-git-guard"],["crates/jeryu-kernel/Cargo.toml","jeryu-kernel"],["crates/jeryu-lifecycle/Cargo.toml","jeryu-lifecycle"],["crates/jeryu-mapcheck/Cargo.toml","jeryu-mapcheck"],["crates/jeryu-obs/Cargo.toml","jeryu-obs"],["crates/jeryu-ops/Cargo.toml","jeryu-ops"],["crates/jeryu-phase11-audit/Cargo.toml","jeryu-phase11-audit"],["bins/jeryu-phase11-bin/Cargo.toml","jeryu-phase11-bin"],["crates/jeryu-phase11-core/Cargo.toml","jeryu-phase11-core"],["crates/jeryu-replay-verifier/Cargo.toml","jeryu-replay-verifier"],["crates/jeryu-repogate/Cargo.toml","jeryu-repogate"],["crates/jeryu-signing/Cargo.toml","jeryu-signing"],["crates/jeryu-signrail/Cargo.toml","jeryu-signrail"],["crates/jeryu-tenant/Cargo.toml","jeryu-tenant"],["crates/jeryu-wsversion/Cargo.toml","jeryu-wsversion"]]' '
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
    else error("jeryu-release-ops owned package set changed") end
' <<< "$cargo_metadata")
mapfile -t owned_packages <<< "$owned_names"
