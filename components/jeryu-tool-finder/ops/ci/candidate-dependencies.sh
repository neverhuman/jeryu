#!/usr/bin/env bash
# Cargo metadata admission after the caller verifies the whole candidate source.
jeryu_candidate_require_intelligence_graph() {
  local root=$1
  [[ $root == /* ]] || return 1
  jq -e --arg root "$root" '
    . as $metadata
    | [.packages[] | select(.name == "jeryu-tool-finder" or
        .name == "jeryu-codegraph" or .name == "jeryu-rustjet")]
      | sort_by(.name) as $packages
    | ($packages | map(.name)) ==
        ["jeryu-codegraph", "jeryu-rustjet", "jeryu-tool-finder"]
      and $metadata.workspace_root == $root
      and ($packages | map(.id) | unique | length) == 3
      and all($packages[]; has("source") and .source == null and
        (.id | type) == "string" and
        (.id as $id | $metadata.workspace_members | index($id)) != null and
        (.id as $id | [$metadata.resolve.nodes[] | select(.id == $id)] | length) == 1)
      and $packages[0].manifest_path ==
        ($root + "/components/jeryu-intelligence/crates/jeryu-codegraph/Cargo.toml")
      and $packages[1].manifest_path ==
        ($root + "/components/jeryu-intelligence/crates/jeryu-rustjet/Cargo.toml")
      and $packages[2].manifest_path ==
        ($root + "/components/jeryu-tool-finder/Cargo.toml")
      and ([$metadata.resolve.nodes[] | select(.id == $packages[2].id)
        | .deps[] | select(.name == "jeryu_codegraph") | .pkg] == [$packages[0].id])
      and ([$metadata.resolve.nodes[] | select(.id == $packages[0].id)
        | .deps[] | select(.name == "jeryu_rustjet") | .pkg] == [$packages[1].id])
      and ([$metadata.resolve.nodes[] | select(.id == $packages[1].id)] | length) == 1
  ' >/dev/null
}
