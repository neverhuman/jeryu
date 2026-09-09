#!/usr/bin/env bash
# Pure metadata contract fixtures; source/receipt qualification is separate.
set -euo pipefail
repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
# shellcheck source=/dev/null
source "$repo_root/ops/ci/candidate-dependencies.sh"
root=/candidate
metadata="$(jq -cn --arg root "$root" '
  {workspace_root:$root,
   workspace_members:["graph", "jet", "finder"],
   packages:[
     {name:"jeryu-codegraph",id:"graph",source:null,
      manifest_path:($root+"/components/jeryu-intelligence/crates/jeryu-codegraph/Cargo.toml")},
     {name:"jeryu-rustjet",id:"jet",source:null,
      manifest_path:($root+"/components/jeryu-intelligence/crates/jeryu-rustjet/Cargo.toml")},
     {name:"jeryu-tool-finder",id:"finder",source:null,
      manifest_path:($root+"/components/jeryu-tool-finder/Cargo.toml")}],
   resolve:{nodes:[
     {id:"finder",deps:[{name:"jeryu_codegraph",pkg:"graph"}]},
     {id:"graph",deps:[{name:"jeryu_rustjet",pkg:"jet"}]},
     {id:"jet",deps:[]}]}}
')"
jeryu_candidate_require_intelligence_graph "$root" <<<"$metadata"
for mutation in \
  '.workspace_root = "/other"' \
  '.packages[0].manifest_path = "/other/Cargo.toml"' \
  '.packages[1].source = "git+https://example.invalid/other"' \
  '.packages += [.packages[0]]' \
  '.packages += [.packages[2]]' \
  '.workspace_members -= ["jet"]' \
  '.resolve.nodes[0].deps[0].pkg = "foreign-graph"' \
  '.resolve.nodes[1].deps[0].pkg = "foreign-jet"' \
  '.resolve.nodes -= [.resolve.nodes[2]]' \
  '.packages[1].id = "graph"' \
  '.resolve.nodes += [.resolve.nodes[0]]' \
  'del(.packages[0].source)'; do
  changed="$(jq -c "$mutation" <<<"$metadata")"
  if jeryu_candidate_require_intelligence_graph "$root" <<<"$changed"; then
    printf 'candidate dependency contract accepted: %s\n' "$mutation" >&2
    exit 1
  fi
done
if jeryu_candidate_require_intelligence_graph /other <<<"$metadata"; then
  printf 'candidate dependency contract accepted a different expected root\n' >&2
  exit 1
fi
printf 'candidate dependency contract tests ok\n'
