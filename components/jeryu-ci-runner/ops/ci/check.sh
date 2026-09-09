#!/usr/bin/env bash
set -euo pipefail

source ops/ci/lib.sh
require_tool jq
expected_release_identity='jeryu-ci-runner-v5.0.0-split.1'
if [[ ! -f VERSION || -L VERSION || "$(stat -c '%h' -- VERSION)" != 1 ||
      "$(wc -l < VERSION)" != 1 || "$(<VERSION)" != "${expected_release_identity}" ||
      "$(tail -c 1 VERSION | od -An -tx1 | tr -d '[:space:]')" != 0a ]]; then
  printf 'VERSION must be the exact one-link release identity %s\n' \
    "${expected_release_identity}" >&2
  exit 1
fi
# Cargo ownership check: the split and monorepo must select the same complete set.
require_tool cargo
require_tool jq
component_root=$(pwd -P)
git_root=$(env -i PATH=/usr/bin:/bin HOME=/nonexistent GIT_CONFIG_GLOBAL=/dev/null \
  GIT_CONFIG_SYSTEM=/dev/null GIT_CONFIG_NOSYSTEM=1 GIT_OPTIONAL_LOCKS=0 \
  GIT_NO_REPLACE_OBJECTS=1 /usr/bin/git -C "$component_root" rev-parse --show-toplevel)
[[ $component_root == "$git_root" || $component_root == "$git_root/components/jeryu-ci-runner" ]] || {
  printf 'Unexpected jeryu-ci-runner workspace location\n' >&2; exit 1;
}
member_manifest="$component_root/crates/jeryu-agent-auth/Cargo.toml"
[[ -f $member_manifest && ! -L $member_manifest &&
   -f $git_root/Cargo.lock && ! -L $git_root/Cargo.lock ]]
cargo_metadata=$(cargo metadata --locked --format-version 1 --no-deps --manifest-path "$member_manifest")
owned_names=$(jq -ser --arg component "$component_root" --arg workspace "$git_root" \
  --argjson expected '[["crates/jeryu-agent-auth/Cargo.toml","jeryu-agent-auth"],["crates/jeryu-agent-stream/Cargo.toml","jeryu-agent-stream"],["crates/jeryu-agentbridge/Cargo.toml","jeryu-agentbridge"],["crates/jeryu-artifact-metadata/Cargo.toml","jeryu-artifact-metadata"],["crates/jeryu-cache-policy/Cargo.toml","jeryu-cache-policy"],["bins/jeryu-ci-bin/Cargo.toml","jeryu-ci-bin"],["crates/jeryu-ci-compiler/Cargo.toml","jeryu-ci-compiler"],["crates/jeryu-ci-governor/Cargo.toml","jeryu-ci-governor"],["crates/jeryu-ci-ir/Cargo.toml","jeryu-ci-ir"],["crates/jeryu-ci-scheduler/Cargo.toml","jeryu-ci-scheduler"],["crates/jeryu-egress/Cargo.toml","jeryu-egress"],["crates/jeryu-phase7-cli/Cargo.toml","jeryu-phase7-cli"],["crates/jeryu-runner-core/Cargo.toml","jeryu-runner-core"],["crates/jeryu-runner-microvm/Cargo.toml","jeryu-runner-microvm"],["crates/jeryu-runner-native/Cargo.toml","jeryu-runner-native"],["crates/jeryu-runner-oci/Cargo.toml","jeryu-runner-oci"],["crates/jeryu-runner-protocol/Cargo.toml","jeryu-runner-protocol"],["crates/jeryu-runner-registry/Cargo.toml","jeryu-runner-registry"],["crates/jeryu-runnerd/Cargo.toml","jeryu-runnerd"],["crates/jeryu-sandbox-linux/Cargo.toml","jeryu-sandbox-linux"]]' '
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
       ($component != $workspace or ($owned | length) == ($members | length)) and all($owned[]; .version == "5.0.0")
    then $owned | sort_by(.name) | .[].name
    else error("jeryu-ci-runner owned package set or release version changed") end
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

if [[ -e repos.manifest.toml || -L repos.manifest.toml ]]; then
  printf 'jeryu-ci-runner may not own an authority repos.manifest.toml\n' >&2
  exit 1
fi
for script in scripts/*.sh ops/ci/*.sh tools/*.sh; do
  [[ -e "$script" ]] || continue
  bash -n "$script"
done
printf 'check ok: %s\n' "$(pwd)"
