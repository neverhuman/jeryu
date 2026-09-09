#!/usr/bin/env bash
# Synthetic report/dispatch tests only: no auditor, Cargo build or real evidence.
set -euo pipefail
root=${3:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)}
helper=${1:-$root/scripts/auxiliary-proofs.sh}
validator=${2:-$root/scripts/auxiliary-rust.jq}
# shellcheck source=/dev/null
source "$helper"
# shellcheck source=/dev/null
source "$root/tests/scratch.sh"
umask 077
temporary=$(mktemp -d -t jeryu-auxiliary-tests.XXXXXXXX)
jeryu_record_test_scratch "$temporary"
cleanup() {
  local result=$?
  if ! jeryu_remove_test_scratch; then
    printf 'retaining auxiliary test scratch: %s\n' "$temporary" >&2
    return 1
  fi
  return "$result"
}
trap cleanup EXIT
passed=0
yes_case() { "$@" || { printf 'expected acceptance: %s\n' "$*" >&2; return 1; }; passed=$((passed + 1)); }
no_case() { if "$@" >/dev/null 2>&1; then printf 'expected refusal: %s\n' "$*" >&2; return 1; fi; passed=$((passed + 1)); }
status_case() {
  local expected=$1 observed=0
  shift
  "$@" >/dev/null 2>&1 || observed=$?
  [[ $observed == "$expected" ]] || {
    printf 'expected exit %s, got %s: %s\n' "$expected" "$observed" "$*" >&2
    return 1
  }
  passed=$((passed + 1))
}
aux_root="$temporary/source"
mkdir "$aux_root" "$temporary/reports"
out="$temporary/reports"

yes_case aux_select independent all
yes_case aux_select migration jeryu-web
no_case aux_select imaginary all
no_case aux_select copy-code ../jeryu-core
no_case aux_select rust-workspace jeryu-core
no_case aux_select full all extra

jq -n --arg root "$aux_root" '{
  schema_version:"1.1.0",generated_by:"jankurai copy-code",repo:$root,status:"pass",
  policy:{strict:true,include_tests:false,active_source_only:true,min_lines:10,min_tokens:100,max_findings:50},
  summary:{files_scanned:4,active_files:3,hard_classes:0,hard_instances:0},classes:[]
}' > "$out/copy.json"
yes_case aux_check_copy_code "$out/copy.json" "$aux_root"
for expression in '.repo="other"' '.status="skipped"' '.policy.strict=false' \
  '.summary.files_scanned="1"' '.summary.active_files=0.5' '.policy.min_tokens=1' \
  '.summary.hard_classes=1' '.summary.hard_instances=2' '.summary.active_files=0' \
  '.classes=[{hard_fail:true,effective_severity:"hard"}]' '.classes=null'; do
  jq "$expression" "$out/copy.json" > "$out/bad-copy.json"
  no_case aux_check_copy_code "$out/bad-copy.json" "$aux_root"
done
jq '.status="review" | .classes=[{hard_fail:false,effective_severity:"warning"}]' \
  "$out/copy.json" > "$out/review.json"
yes_case aux_check_copy_code "$out/review.json" "$aux_root"
ln -s "$out/copy.json" "$out/link.json"
no_case aux_check_copy_code "$out/link.json" "$aux_root"
[[ -L $out/link.json && $(readlink "$out/link.json") == "$out/copy.json" ]]
unlink "$out/link.json"
ln "$out/copy.json" "$out/hardlink.json"
no_case aux_check_copy_code "$out/copy.json" "$aux_root"
[[ $(stat -c %i "$out/copy.json") == "$(stat -c %i "$out/hardlink.json")" ]]
unlink "$out/hardlink.json"

jq -n --arg root "$aux_root" '{
  schema_version:"1.0.0",command:"jankurai migrate",source_root:$root,status:"complete",
  inventory:{},module_inventory:[],liability_score:20,required_proof_lanes:[],rollback_cutover_notes:[]
}' > "$out/migration.json"
yes_case aux_check_migration "$out/migration.json" "$aux_root"
for expression in '.source_root="other"' '.status="not-applicable"' '.inventory=null' \
  '.liability_score=-1' '.liability_score=101' '.required_proof_lanes=null'; do
  jq "$expression" "$out/migration.json" > "$out/bad-migration.json"
  no_case aux_check_migration "$out/bad-migration.json" "$aux_root"
done

jq -n --arg root "$aux_root" '
  ["jeryu-core","jeryu-intelligence","jeryu-release-ops","jeryu-deploy",
    "jeryu-cache","jeryu-ci-runner","jeryu-jira","jeryu-tool","jeryu-tool-finder"] as $owners |
  [range(65) | . as $i | {id:("id"+tostring),name:("jeryu-fixture-"+tostring),
    version:"1.0.0",source:null,
    manifest_path:($root+"/components/"+$owners[$i % 9]+"/crates/fixture-"+tostring+"/Cargo.toml")
  }] as $p |
  {workspace_root:$root,workspace_members:[$p[].id],packages:$p,
    resolve:{nodes:[$p | to_entries[] | {id:.value.id,
      deps:[{pkg:$p[((.key+1) % 65)].id}]}]}}
' > "$out/metadata.json"
yes_case jq -e --arg root "$aux_root" -f "$validator" "$out/metadata.json" > "$out/workspace-ownership.json"
yes_case jq -e '.components["jeryu-web"] == [] and .workspace_member_count == 65 and
  all(.members[]; (.direct_dependencies | length) == 1 and (.reverse_dependencies | length) == 1)' \
  "$out/workspace-ownership.json" >/dev/null
for expression in '.workspace_root="other"' '.workspace_members|=.[1:]' \
  '.workspace_members[0]=.workspace_members[1]' '.resolve.nodes|=.[1:]' \
  '.resolve.nodes[0]=.resolve.nodes[1]' '.packages[0].source="git+other"' \
  '.packages[0].manifest_path|=sub("jeryu-core";"jeryu-unknown")' \
  '.packages[0].manifest_path|=sub("/crates/";"/../")'; do
  jq "$expression" "$out/metadata.json" > "$out/bad-metadata.json"
  no_case jq -e --arg root "$aux_root" -f "$validator" "$out/bad-metadata.json"
done
jq '{workspace_root,members:[.members[] | {name,manifest_path,direct_dependencies,reverse_dependencies}]}' \
  "$out/workspace-ownership.json" > "$out/agent-map.json"
jq '{workspace_root,entries:[.members[] | {arc:.name}]}' "$out/workspace-ownership.json" > "$out/test-map.json"
jq '{workspace_root,crates:[.members[] | {name,direct_deps:.direct_dependencies,
  reverse_deps:.reverse_dependencies,interface_hash:("a"*64),implementation_hash:("b"*64),file_count:1}]}' \
  "$out/workspace-ownership.json" > "$out/witness-graph.json"
yes_case aux_check_rust_workspace "$out"
cp "$out/witness-graph.json" "$temporary/witness-original.json"
for expression in '.workspace_root="other"' '.crates|=.[1:]' \
  '.crates[0].direct_deps=[]' '.crates[0].reverse_deps=[]' \
  '.crates[0].interface_hash=""' '.crates[0].file_count=0' \
  '.crates[0].file_count="1"' '.crates[0].file_count=1.5'; do
  jq "$expression" "$temporary/witness-original.json" > "$out/witness-graph.json"
  no_case aux_check_rust_workspace "$out"
done
cp "$temporary/witness-original.json" "$out/witness-graph.json"
jq '.members[0].manifest_path="wrong/Cargo.toml"' "$out/agent-map.json" > "$temporary/wrong-map.json"
cp "$temporary/wrong-map.json" "$out/agent-map.json"
no_case aux_check_rust_workspace "$out"

# Empty stdout from a failed Git read must stop source admission.
require_jankurai() { return 0; }
aux_git() { return 128; }
no_case aux_source

# Real step aggregation retains a failed command and continues only when source is stable.
aux_attempt="$temporary/attempt"
mkdir "$aux_attempt"
# Used by aux_step from the sourced helper.
# shellcheck disable=SC2034
aux_initial=stable
aux_failed=0
aux_source() { printf '%s\n' stable; }
synthetic_failure() { printf 'synthetic failed output\n' > "$2/diagnostic.txt"; return 23; }
synthetic_success() { printf 'synthetic completed output\n' > "$2/diagnostic.txt"; }
yes_case aux_step synthetic-failure jeryu-core synthetic_failure
yes_case test "$aux_failed" -eq 1
yes_case jq -e '.exit_code == 23 and .passed == false' "$aux_attempt/steps.jsonl" >/dev/null
yes_case aux_step synthetic-success jeryu-core synthetic_success
yes_case test "$(wc -l < "$aux_attempt/steps.jsonl")" -eq 2
aux_source() { printf '%s\n' changed; }
no_case aux_step source-changed jeryu-core synthetic_success
yes_case test "$(wc -l < "$aux_attempt/steps.jsonl")" -eq 2
# Used by aux_conclude from the sourced helper.
# shellcheck disable=SC2034
aux_producer=independent
no_case aux_conclude
aux_failed=0
yes_case aux_conclude
# shellcheck disable=SC2034
aux_producer=full
no_case aux_conclude
yes_case rg -q 'authenticated protected predecessor' "$aux_attempt/unavailable-full-gates.txt"

# Test dispatch with functions only. Production admission has no mock-tool flag.
aux_step() { printf '%s %s\n' "$1" "$2" >> "$temporary/dispatch.txt"; }
: > "$temporary/dispatch.txt"
aux_select independent all
aux_dispatch
yes_case test "$(wc -l < "$temporary/dispatch.txt")" -eq 9
yes_case test "$(rg -c '^rust-workspace jeryu$' "$temporary/dispatch.txt")" -eq 1
: > "$temporary/dispatch.txt"
aux_select full jeryu-web
aux_dispatch
yes_case test "$(wc -l < "$temporary/dispatch.txt")" -eq 2
no_case rg '^rust-workspace ' "$temporary/dispatch.txt"
yes_case test -n "$(aux_missing_full)"

# A producer failure cannot become a synthesized empty report.
jankurai() { return 23; }
mkdir "$temporary/failed"
status_case 23 aux_copy_code jeryu-core "$temporary/failed"
no_case test -e "$temporary/failed/copy-code.json"
status_case 23 aux_migration jeryu-core "$temporary/failed"
no_case test -e "$temporary/failed/migration-report.json"
# Exercise each Rust producer's status through real helper control flow.
# The compiler and auditor below are shell functions restricted to this test.
metadata_fixture="$out/metadata.json"
mkdir "$aux_root/scripts"
cp -- "$validator" "$aux_root/scripts/auxiliary-rust.jq"
while IFS= read -r manifest; do
  mkdir -p -- "$(dirname -- "$aux_root/$manifest")"
  printf 'synthetic owning manifest\n' > "$aux_root/$manifest"
done < <(jq -r '.members[].manifest_path' "$out/workspace-ownership.json")
aux_git() { printf '%040d\n' 1; }
cargo() {
  case $1 in
    --version)
      [[ $failure_stage != cargo-version ]] || return 23
      printf 'cargo synthetic-version\n' ;;
    metadata)
      [[ $failure_stage != cargo-metadata ]] || return 23
      cat -- "$metadata_fixture" ;;
    *) return 99 ;;
  esac
}
rustc() {
  [[ $failure_stage != rustc-version ]] || return 23
  printf 'rustc synthetic-version\n'
}
jankurai() {
  [[ $1 == rust ]] || return 99
  [[ $2 != map || $failure_stage != map ]] || return 23
  [[ $2 != witness || $failure_stage != witness ]] || return 23
}
for failure_stage in cargo-version rustc-version cargo-metadata map witness; do
  mkdir -- "$temporary/$failure_stage"
  status_case 23 aux_rust_workspace jeryu "$temporary/$failure_stage"
done
failure_stage=missing-reports
mkdir -- "$temporary/$failure_stage"
# An exit-zero tool without its promised outputs still cannot pass.
status_case 1 aux_rust_workspace jeryu "$temporary/$failure_stage"
printf 'Auxiliary synthetic checks passed: %s\n' "$passed"
