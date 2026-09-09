#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT

printf 'fixture redline manifest\n' >"$tmp/redline.manifest.toml"
printf 'fixture redline policy\n' >"$tmp/redline.policy.toml"
printf '%s\n' \
  '{"manifest_sha256":"0000000000000000000000000000000000000000000000000000000000000000","repositories":[],"status":"fail"}' \
  >"$tmp/family-ci.json"
family_digest="$(sha256sum "$tmp/family-ci.json" | awk '{print $1}')"
printf '%s  %s\n' "$family_digest" family-ci.json >"$tmp/family-ci.json.sha256"

common_args=(
  --family-ci "$tmp/family-ci.json"
  --redline-manifest "$tmp/redline.manifest.toml"
  --redline-policy "$tmp/redline.policy.toml"
  --consumer-policy "$root/agent/audit-policy.toml"
)

# The real authority must pass the producer's identity gate. A topic branch
# stops at the reviewed-main fence; protected main reaches the deliberately
# mismatched fixture receipt. Either result proves authority validation passed.
if bash "$root/ops/ci/redline-consumer.sh" \
  "${common_args[@]}" \
  --consumer-manifest "$root/repos.manifest.toml" \
  --output "$tmp/canonical.json" >"$tmp/canonical.log" 2>&1; then
  printf 'canonical producer fixture unexpectedly emitted evidence\n' >&2
  exit 1
fi
if grep -Eq 'consumer manifest path is not|Jeryu manifest authority is invalid' \
  "$tmp/canonical.log"; then
  printf 'canonical Jeryu authority failed producer validation\n' >&2
  cat "$tmp/canonical.log" >&2
  exit 1
fi
grep -Eq 'evidence must be generated from reviewed main|family CI manifest hash does not match' \
  "$tmp/canonical.log"

cat >"$tmp/fabricated.manifest.toml" <<'EOF'
repo_family = "jeryu-split"
manifest_authority = "/tmp/fabricated.manifest.toml"
[[repo]]
name = "jeryu-release-ops"
EOF
if bash "$root/ops/ci/redline-consumer.sh" \
  "${common_args[@]}" \
  --consumer-manifest "$tmp/fabricated.manifest.toml" \
  --output "$tmp/fabricated.json" >"$tmp/fabricated.log" 2>&1; then
  printf 'fabricated consumer authority was accepted\n' >&2
  exit 1
fi
grep -Fq 'consumer manifest path is not the canonical Jeryu authority' \
  "$tmp/fabricated.log"
[[ ! -e "$tmp/fabricated.json" ]]

printf 'Redline consumer authority producer path ok\n'
