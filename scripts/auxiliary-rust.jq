# Cargo's complete default-feature graph is the authority for package ownership
# and cross-component edges. This is a workspace graph, including Web's empty set.
def require($test; $message):
  if $test then . else error($message) end;
[
  "jeryu-cache","jeryu-ci-runner","jeryu-core","jeryu-deploy",
  "jeryu-intelligence","jeryu-jira","jeryu-release-ops","jeryu-tool",
  "jeryu-tool-finder","jeryu-web"
] as $components |
. as $metadata |
require(.workspace_root == $root; "metadata belongs to another workspace") |
require((.workspace_members | type) == "array" and
  (.workspace_members | length) == 65 and
  (.workspace_members | unique | length) == 65; "expected all 65 unique workspace members") |
.workspace_members as $ids |
[.packages[] | select(.id as $id | $ids | index($id))] as $packages |
require(($packages | length) == 65 and
  ($packages | map(.name) | unique | length) == 65; "missing or duplicate package identity") |
require(all($packages[]; .source == null and
  (.manifest_path | startswith($root + "/components/")) and
  (.manifest_path | ltrimstr($root + "/") | test("^components/jeryu-[a-z-]+/([A-Za-z0-9_-]+/)*Cargo.toml$")) and
  (.manifest_path | test("[\\n\\r\\t]") | not)); "nonlocal or invalid owning manifest") |
require((.resolve.nodes | type) == "array"; "resolved dependency graph is missing") |
[.resolve.nodes[] | select(.id as $id | $ids | index($id))] as $nodes |
require(($nodes | map(.id) | sort) == ($ids | sort); "missing or duplicate resolved workspace node") |
($packages | map({key:.id,value:.name}) | from_entries) as $names |
[$packages[] | . as $p |
  ($p.manifest_path | ltrimstr($root + "/")) as $manifest |
  ($manifest | split("/")[1]) as $component |
  require(($components | index($component)) != null; "unknown owning component") |
  {
    name:$p.name,id:$p.id,version:$p.version,component:$component,manifest_path:$manifest,
    direct_dependencies:([$nodes[] | select(.id == $p.id) | .deps[] |
      select(.pkg as $id | $ids | index($id)) | $names[.pkg]] | unique)
  }
] as $members |
[$members[] | . as $m |
  . + {reverse_dependencies:([$members[] |
    select(.direct_dependencies | index($m.name)) | .name] | sort)}
] | sort_by(.name) as $members |
{
  schema:"jeryu.auxiliary-workspace/v1",workspace_root:$root,
  cargo_feature_configuration:"default",workspace_member_count:65,members:$members,
  components:($components | map(. as $c |
    {key:$c,value:[$members[] | select(.component == $c) | .name]}) | from_entries)
}
