#!/usr/bin/env bash
set -euo pipefail
source ops/ci/lib.sh
cd "$REPO_ROOT"
require_jankurai
require_tool git
require_tool jq

proof_base="${JAIN_CONTRACT_BASE_REF:-${JAIN_PROOF_BASE_REF:-origin/main}}"
git rev-parse --verify "$proof_base^{commit}" >/dev/null
git merge-base --is-ancestor "$proof_base" HEAD || {
  printf 'proof base is not an ancestor of HEAD: %s\n' "$proof_base" >&2
  exit 1
}
[[ "$(git rev-parse "$proof_base")" != "$(git rev-parse HEAD)" ]] || {
  printf 'proof base resolves to HEAD: %s\n' "$proof_base" >&2
  exit 1
}

prepare_output_directory() {
  local path="$1"
  local root_dev mode
  if [[ -L "$path" || ( -e "$path" && ! -d "$path" ) ]]; then
    printf 'proof output path is not a physical directory: %s\n' "$path" >&2
    exit 1
  fi
  if [[ ! -e "$path" ]]; then
    mkdir -m 0750 -- "$path"
  fi
  [[ "$(realpath -e -- "$path")" == "$REPO_ROOT"/target* ]] || {
    printf 'proof output escaped repository target: %s\n' "$path" >&2
    exit 1
  }
  root_dev="$(stat -Lc '%d' -- "$REPO_ROOT")"
  [[ "$(stat -Lc '%d:%u' -- "$path")" == "$root_dev:$EUID" ]] || {
    printf 'proof output directory has unsafe device or owner: %s\n' "$path" >&2
    exit 1
  }
  mode="$(stat -Lc '%a' -- "$path")"
  (( (8#$mode & 8#002) == 0 )) || {
    printf 'proof output directory is world writable: %s\n' "$path" >&2
    exit 1
  }
}

prepare_output_file() {
  local path="$1"
  if [[ -L "$path" || ( -e "$path" && ! -f "$path" ) ]]; then
    printf 'proof output is not a physical regular file: %s\n' "$path" >&2
    exit 1
  fi
  if [[ -e "$path" ]]; then
    [[ "$(stat -Lc '%u:%h' -- "$path")" == "$EUID:1" ]] || {
      printf 'proof output has unsafe owner or link count: %s\n' "$path" >&2
      exit 1
    }
  else
    (umask 077; set -o noclobber; : >"$path")
  fi
}

prepare_output_directory "$REPO_ROOT/target"
prepare_output_directory "$REPO_ROOT/target/jankurai"
prepare_output_directory "$REPO_ROOT/target/jankurai/proof-routing"

plan=target/jankurai/proof-plan.json
plan_md=target/jankurai/proof-plan.md
blocked=target/jankurai/proof-routing/blocked-plan.json
malformed=target/jankurai/proof-routing/malformed-plan.json
risk=target/jankurai/proof-routing/risk-plan.json
unknown_decision=target/jankurai/proof-routing/unknown-decision-plan.json
duplicate_path=target/jankurai/proof-routing/duplicate-path-plan.json
malformed_types=target/jankurai/proof-routing/malformed-types-plan.json
omitted_path=target/jankurai/proof-routing/omitted-path-plan.json
for output in "$plan" "$plan_md" "$blocked" "$malformed" "$risk" \
  "$unknown_decision" "$duplicate_path" "$malformed_types" "$omitted_path"; do
  prepare_output_file "$output"
done

GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.abbrev GIT_CONFIG_VALUE_0=40 \
  jankurai proof . --changed-from "$proof_base" --out "$plan" --md "$plan_md"
expected_paths_json="$(git diff --name-only -z "$proof_base"...HEAD \
  | jq -Rs 'split("\u0000")[:-1] | sort')"
[[ "$(jq 'length' <<<"$expected_paths_json")" -gt 0 ]] || {
  printf 'protected-base diff contains no changed paths\n' >&2
  exit 1
}

validate_plan() {
  local candidate="$1"
  local head
  head="$(git rev-parse HEAD)"
  jq -e --arg head "$head" --arg base "$proof_base" \
    --argjson expected_paths "$expected_paths_json" '
    .schema_version == "1.0.0" and
    .repo_root == "." and
    .git_head == $head and
    .base_ref == $base and
    (.changed_paths | type) == "array" and
    (.changed_paths | length) > 0 and
    all(.changed_paths[]; type == "string" and length > 0) and
    (.changed_paths | unique | length) == (.changed_paths | length) and
    (.changed_paths | sort) == $expected_paths and
    (.route_decisions | type) == "array" and
    (.route_decisions | length) == (.changed_paths | length) and
    all(.route_decisions[];
      type == "object" and
      (.changed_path | type) == "string" and
      (.decision | type) == "string" and
      (.reason | type) == "string") and
    ([.route_decisions[].changed_path] | sort) == (.changed_paths | sort) and
    ([.route_decisions[].changed_path] | unique | length) == (.route_decisions | length) and
    all(.route_decisions[]; .decision == "pass") and
    all(.route_decisions[]; (.reason | contains("not a named proof lane") | not)) and
    (.commands | type) == "array" and
    (.commands | length) > 0 and
    all(.commands[]; type == "string" and length > 0) and
    (.commands | unique | length) == (.commands | length) and
    (.risk_notes | type) == "array" and (.risk_notes | length) == 0 and
    (.human_approval_requirements | type) == "array" and
    (.human_approval_requirements | length) == 0
  ' "$candidate" >/dev/null
}

validate_plan "$plan"

# Prove the validator rejects every previously fail-open plan shape.
jq '.route_decisions[0].decision = "blocked"' "$plan" >"$blocked"
if validate_plan "$blocked"; then
  printf 'proof routing accepted an injected blocked route\n' >&2
  exit 1
fi
printf '{}\n' >"$malformed"
if validate_plan "$malformed"; then
  printf 'proof routing accepted a malformed plan\n' >&2
  exit 1
fi
jq '.risk_notes = ["injected release risk"]' "$plan" >"$risk"
if validate_plan "$risk"; then
  printf 'proof routing accepted injected release risk\n' >&2
  exit 1
fi
jq '.route_decisions[0].decision = "review"' "$plan" >"$unknown_decision"
if validate_plan "$unknown_decision"; then
  printf 'proof routing accepted an unknown decision\n' >&2
  exit 1
fi
jq '.changed_paths += [.changed_paths[0]] | .route_decisions += [.route_decisions[0]]' \
  "$plan" >"$duplicate_path"
if validate_plan "$duplicate_path"; then
  printf 'proof routing accepted duplicate changed paths\n' >&2
  exit 1
fi
jq '.commands = {"fake":"just check"} | .risk_notes = {} | .human_approval_requirements = {}' \
  "$plan" >"$malformed_types"
if validate_plan "$malformed_types"; then
  printf 'proof routing accepted malformed inventory types\n' >&2
  exit 1
fi
jq 'del(.changed_paths[0]) | del(.route_decisions[0])' "$plan" >"$omitted_path"
if validate_plan "$omitted_path"; then
  printf 'proof routing accepted a plan omitting a changed path\n' >&2
  exit 1
fi

for output in "$plan" "$plan_md" "$blocked" "$malformed" "$risk" \
  "$unknown_decision" "$duplicate_path" "$malformed_types" "$omitted_path"; do
  [[ -f "$output" && ! -L "$output" && "$(stat -Lc '%u:%h' -- "$output")" == "$EUID:1" ]]
done
printf 'proof routing ok: changed=%s commands=%s fail-closed-hostiles=7\n' \
  "$(jq -r '.changed_paths | length' "$plan")" \
  "$(jq -r '.commands | length' "$plan")"
