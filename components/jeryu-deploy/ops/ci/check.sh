#!/usr/bin/env bash
set -euo pipefail

source ops/ci/lib.sh
# Cargo ownership check: the split and monorepo must select the same complete set.
require_tool cargo
require_tool jq
component_root=$(pwd -P)
git_root=$(env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
  GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_OPTIONAL_LOCKS=0 \
  GIT_NO_REPLACE_OBJECTS=1 /usr/bin/git -C "$component_root" rev-parse --show-toplevel)
[[ $component_root == "$git_root" || $component_root == "$git_root/components/jeryu-deploy" ]] || {
  printf 'Unexpected jeryu-deploy workspace location\n' >&2; exit 1;
}
member_manifest="$component_root/crates/jeryu-api/Cargo.toml"
[[ -f $member_manifest && ! -L $member_manifest &&
   -f $git_root/Cargo.lock && ! -L $git_root/Cargo.lock ]]
cargo_metadata=$(cargo metadata --locked --format-version 1 --no-deps --manifest-path "$member_manifest")
owned_names=$(jq -ser --arg component "$component_root" --arg workspace "$git_root" \
  --argjson expected '[["crates/jeryu-api/Cargo.toml","jeryu-api"],["crates/jeryu-cli/Cargo.toml","jeryu-cli"],["crates/jeryu-split-tool/Cargo.toml","jeryu-split-tool"]]' '
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
    else error("jeryu-deploy owned package set or release version changed") end
' <<< "$cargo_metadata")
mapfile -t owned_packages <<< "$owned_names"
if [[ "${JERYU_SPLIT_FULL_CHECK:-0}" == "1" ]]; then
  check_scope=(--workspace)
  if [[ $component_root != "$git_root" ]]; then
    check_scope=()
    for package in "${owned_packages[@]}"; do check_scope+=(--package "$package"); done
  fi
  cargo check --locked --manifest-path "$member_manifest" "${check_scope[@]}" \
    --all-targets --jobs "${JERYU_CI_JOBS:-40}"
fi
# End Cargo ownership check.

if [[ -f package.json ]]; then
  node -e 'JSON.parse(require("fs").readFileSync("package.json", "utf8"))' >/dev/null
  if [[ -f apps/web/package.json ]]; then
    node -e 'JSON.parse(require("fs").readFileSync("apps/web/package.json", "utf8"))' >/dev/null
  fi
  if [[ "${JERYU_SPLIT_FULL_CHECK:-0}" == "1" ]]; then
    npm --workspace @jeryu/web run typecheck
  fi
fi

if [[ -f repos.manifest.toml ]]; then
  cargo run --locked --offline --quiet -p jeryu-split-tool --bin jeryu-split -- \
    manifest --manifest repos.manifest.toml >/dev/null
fi
for script in scripts/*.sh ops/ci/*.sh ops/deploy/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
bash scripts/check-agent-maps.sh
bash scripts/test-ci-phases.sh
bash scripts/test-coverage-evidence.sh
bash scripts/test-workspace-lock.sh
bash scripts/test-web-build.sh
printf 'check ok: %s\n' "$(pwd)"
