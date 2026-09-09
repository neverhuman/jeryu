#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
family_ci=""
redline_manifest=""
redline_policy=""
consumer_manifest=""
consumer_policy="$root/agent/audit-policy.toml"
output=""
engine_tag="redline-core-v4.1.0-jain.6"
tool_version="jeryu-redline-consumer/v1"
evidence_schema="$root/schemas/redline-consumer-evidence.schema.json"

die() {
  printf 'redline-consumer: %s\n' "$*" >&2
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --family-ci)
      family_ci="${2:?--family-ci requires a path}"
      shift 2
      ;;
    --redline-manifest)
      redline_manifest="${2:?--redline-manifest requires a path}"
      shift 2
      ;;
    --redline-policy)
      redline_policy="${2:?--redline-policy requires a path}"
      shift 2
      ;;
    --consumer-manifest)
      consumer_manifest="${2:?--consumer-manifest requires a path}"
      shift 2
      ;;
    --consumer-policy)
      consumer_policy="${2:?--consumer-policy requires a path}"
      shift 2
      ;;
    --output)
      output="${2:?--output requires a path}"
      shift 2
      ;;
    *)
      die "unsupported argument: $1"
      ;;
  esac
done

[[ -n "$family_ci" ]] || die "--family-ci is required"
[[ -n "$redline_manifest" ]] || die "--redline-manifest is required"
[[ -n "$redline_policy" ]] || die "--redline-policy is required"
[[ -n "$consumer_manifest" ]] || die "--consumer-manifest is required"
[[ -n "$output" ]] || die "--output is required"

for command in cargo date git jq python3 realpath sha256sum; do
  command -v "$command" >/dev/null 2>&1 || die "required command is unavailable: $command"
done

[[ -f "$family_ci" && -f "$family_ci.sha256" ]] || die "family CI receipt or sidecar is missing"
[[ -f "$redline_manifest" ]] || die "Redline manifest is missing"
[[ -f "$redline_policy" ]] || die "Redline policy is missing"
[[ -f "$consumer_manifest" ]] || die "Jeryu consumer manifest is missing"
[[ -f "$consumer_policy" ]] || die "Jeryu consumer policy is missing"
[[ -f "$evidence_schema" ]] || die "consumer evidence schema is missing"

canonical_consumer_manifest="$(realpath -e "$root/repos.manifest.toml")"
provided_consumer_manifest="$(realpath -e "$consumer_manifest")"
[[ "$provided_consumer_manifest" == "$canonical_consumer_manifest" ]] \
  || die "consumer manifest path is not the canonical Jeryu authority"

cargo run --locked --quiet --manifest-path "$root/Cargo.toml" \
  -p jeryu-repogate -- --root "$root" family-manifest >/dev/null \
  || die "Jeryu manifest authority is invalid"

python3 - "$consumer_policy" <<'PY' || die "Jeryu consumer policy identity is invalid"
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

policy = tomllib.loads(Path(sys.argv[1]).read_text())
if policy.get("workspace") != "jeryu-release-ops":
    raise SystemExit("consumer policy does not bind jeryu-release-ops")
PY

branch="$(git -C "$root" branch --show-current)"
[[ "$branch" == "main" ]] || die "evidence must be generated from reviewed main, got $branch"
[[ -z "$(git -C "$root" status --porcelain=v1 --untracked-files=all)" ]] || die "consumer worktree is dirty"
source_commit="$(git -C "$root" rev-parse HEAD)"
forge_commit="$(git -C "$root" ls-remote origin refs/heads/main | awk 'NR == 1 { print $1 }')"
[[ "$source_commit" == "$forge_commit" ]] || die "consumer HEAD does not equal forge main"

family_digest="$(sha256sum "$family_ci" | awk '{ print $1 }')"
sidecar_digest="$(awk 'NR == 1 { print $1 }' "$family_ci.sha256")"
[[ "$family_digest" == "$sidecar_digest" ]] || die "family CI checksum sidecar does not match"

manifest_digest="$(sha256sum "$redline_manifest" | awk '{ print $1 }')"
family_manifest_digest="$(jq -er '.manifest_sha256' "$family_ci")"
[[ "$manifest_digest" == "$family_manifest_digest" ]] || die "family CI manifest hash does not match Redline manifest"
[[ "$(jq -er '.status' "$family_ci")" == "pass" ]] || die "Redline family CI is not passing"

engine_commit="$(jq -er --arg tag "$engine_tag" '.repositories[] | select(.name == "redline-core" and .status == "pass" and .tag == $tag) | .commit' "$family_ci")"
[[ "$engine_commit" =~ ^[0-9a-f]{40}$ ]] || die "family CI does not bind the Redline engine commit"

contract_manifest="$root/tests/redline/Cargo.toml"
locked_source="$(cargo metadata --locked --format-version 1 --manifest-path "$contract_manifest" | jq -er '.packages[] | select(.name == "redlinedb") | .source')"
[[ "$locked_source" == *"tag=$engine_tag"*"#$engine_commit" ]] || die "Cargo.lock does not bind the reviewed Redline tag and commit"

mkdir -p "$(dirname "$output")"
test_log="${output%.json}.test.log"
if ! cargo test --locked --manifest-path "$contract_manifest" --test redline_consumer_contract -- --nocapture >"$test_log" 2>&1; then
  printf 'redline-consumer: contract test failed; see %s\n' "$test_log" >&2
  exit 1
fi

generated_at="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
policy_digest="$(sha256sum "$redline_policy" | awk '{ print $1 }')"
consumer_manifest_digest="$(sha256sum "$consumer_manifest" | awk '{ print $1 }')"
consumer_policy_digest="$(sha256sum "$consumer_policy" | awk '{ print $1 }')"
test_log_digest="$(sha256sum "$test_log" | awk '{ print $1 }')"
test_log_record="$(basename "$test_log")"
proof_lock_id="redline-proof/v2/4.1.0/$engine_commit"

jq -n \
  --arg schema_version "redline.consumer-evidence/v1" \
  --arg consumer "jeryu-split" \
  --arg family "redline-split" \
  --arg generated_at "$generated_at" \
  --arg status "pass" \
  --arg source_commit "$source_commit" \
  --arg required_check "jeryu-split/redline-consumer" \
  --arg engine_tag "$engine_tag" \
  --arg engine_commit "$engine_commit" \
  --arg proof_lock_id "$proof_lock_id" \
  --arg family_ci_receipt_sha256 "$family_digest" \
  --arg manifest_sha256 "$manifest_digest" \
  --arg policy_sha256 "$policy_digest" \
  --arg consumer_manifest_sha256 "$consumer_manifest_digest" \
  --arg consumer_policy_sha256 "$consumer_policy_digest" \
  --arg test_log "$test_log_record" \
  --arg test_log_sha256 "$test_log_digest" \
  --arg tool_version "$tool_version" \
  '{schema_version:$schema_version,consumer:$consumer,family:$family,generated_at:$generated_at,status:$status,source_commit:$source_commit,required_check:$required_check,engine_tag:$engine_tag,engine_commit:$engine_commit,proof_lock_id:$proof_lock_id,family_ci_receipt_sha256:$family_ci_receipt_sha256,manifest_sha256:$manifest_sha256,policy_sha256:$policy_sha256,consumer_manifest_sha256:$consumer_manifest_sha256,consumer_policy_sha256:$consumer_policy_sha256,test_log:$test_log,test_log_sha256:$test_log_sha256,tool_version:$tool_version}' \
  >"$output"

jq -e --slurpfile schema "$evidence_schema" '
  . as $evidence
  | $schema[0] as $contract
  | (($evidence | keys | sort) == ($contract.required | sort))
    and ([
      $contract.properties
      | to_entries[]
      | select(.value.const != null) as $property
      | $evidence[$property.key] == $property.value.const
    ] | all)
    and ([
      $contract.properties
      | to_entries[]
      | select(.value.pattern != null) as $property
      | ($evidence[$property.key] | type == "string" and test($property.value.pattern))
    ] | all)
' "$output" >/dev/null || die "generated evidence does not match its closed schema"

output_digest="$(sha256sum "$output" | awk '{ print $1 }')"
printf '%s  %s\n' "$output_digest" "$(basename "$output")" >"$output.sha256"
printf 'Jeryu Redline consumer evidence: %s\n' "$output"
