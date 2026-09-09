#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$repo_root"
source "$repo_root/ops/ci/lib.sh"
source "$repo_root/ops/ci/source-authority.sh"
for tool in jq cargo gitleaks actionlint cargo-audit cargo-deny syft; do
  require_tool "$tool"
done

die() {
  printf 'security check failed: %s\n' "$*" >&2
  exit 1
}

prepare_dir() {
  local relative="$1"
  local path="$repo_root/$relative"
  [[ ! -L "$path" ]] || die "$relative must not be a symlink"
  mkdir -p -- "$path"
  [[ -d "$path" ]] || die "$relative is not a directory"
  [[ "$(realpath -e -- "$path")" == "$path" ]] ||
    die "$relative escapes the repository"
}

require_output_slot() {
  local relative="$1"
  local path="$repo_root/$relative"
  if [[ -e "$path" || -L "$path" ]]; then
    [[ -f "$path" && ! -L "$path" && "$(stat -c '%h' -- "$path")" -eq 1 &&
       "$(realpath -e -- "$path")" == "$path" ]] ||
      die "$relative lacks physical single-link custody"
  fi
}

jeryu_source_snapshot
head_sha="$JERYU_SOURCE_HEAD"
tree_sha="$JERYU_SOURCE_TREE"
source_inputs_sha="$JERYU_SOURCE_INPUTS_SHA256"
source_scope_json="$JERYU_SOURCE_SCOPE_JSON"
security_schema='jeryu.split.security/v1'
cargo_lock_path="$repo_root/Cargo.lock"
syft_scope=(dir:. --exclude './target/**' --exclude './.git/**')
if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
  candidate_root="$(jeryu_candidate_source_root)"
  security_schema='jeryu.monorepo-candidate.security/v1'
  cargo_lock_path="$candidate_root/Cargo.lock"
  syft_scope=("dir:$candidate_root" --exclude '**/target/**' --exclude '**/.git/**')
fi

prepare_dir target
prepare_dir target/security
require_output_slot target/security/evidence.json
require_output_slot target/security/cargo-audit.json
require_output_slot target/security/jeryu-tool-finder.spdx.json
audit_tmp=''
sbom_tmp=''
evidence_tmp=''
cleanup_temps() {
  rm -f -- "${audit_tmp:-}" "${sbom_tmp:-}" "${evidence_tmp:-}"
}
trap cleanup_temps EXIT
gitleaks_bin="$(command -v gitleaks)"
jeryu_with_scrubbed_git "$gitleaks_bin" detect --redact --verbose
if [[ -d .github/workflows ]]; then
  actionlint .github/workflows/*.yml
fi
if [[ ${JERYU_MONOREPO_CANDIDATE:-0} == 1 ]]; then
  if ! jeryu_candidate_root_git "$candidate_root" ls-files -z |
    while IFS= read -r -d '' path; do
      [[ ${path##*/} != .env ]] || exit 1
    done; then
    die 'tracked monorepo environment file or failed source inventory'
  fi
else
  if find . -path './.git' -prune -o -path './target' -prune -o -name '.env' -type f -print | grep -q .; then
    printf 'security check failed: committed .env file found\n' >&2
    exit 1
  fi
fi
# Dependency surface must stay parseable (supply-chain sanity, offline).
if [[ -f Cargo.toml ]]; then
  cargo_bin="$(command -v cargo)"
  jeryu_with_scrubbed_git "$cargo_bin" metadata --locked --offline --format-version 1 --no-deps >/dev/null
fi
# License, ban, and source policy is mandatory and does not fetch. Advisory
# freshness is enforced separately by the fail-closed cached cargo-audit step.
cargo_deny_bin="$(command -v cargo-deny)"
jeryu_with_scrubbed_git /usr/bin/env CARGO="$cargo_bin" \
  "$cargo_deny_bin" check bans licenses sources --disable-fetch
cargo_audit_status="skipped-no-lock"
if [[ -f "$cargo_lock_path" ]]; then
  cargo_audit_bin="$(command -v cargo-audit)"
  audit_tmp="$(mktemp "$repo_root/target/security/.cargo-audit.XXXXXX")"
  if jeryu_with_scrubbed_git "$cargo_audit_bin" \
    audit --file "$cargo_lock_path" --no-fetch --format json > "$audit_tmp" 2>/dev/null; then
    cargo_audit_status="clean"
  else
    cargo_audit_status="findings-or-offline-db-unavailable"
  fi
  if jq -e 'type == "object"' "$audit_tmp" >/dev/null 2>&1; then
    chmod 0600 "$audit_tmp"
  else
    rm -f -- "$audit_tmp"
    cargo_audit_status="invalid-output"
  fi
fi
sbom_status="skipped-tool-unavailable"
syft_bin="$(command -v syft)"
sbom_tmp="$(mktemp "$repo_root/target/security/.sbom.XXXXXX")"
if jeryu_with_scrubbed_git "$syft_bin" "${syft_scope[@]}" \
  -o "spdx-json=$sbom_tmp" >/dev/null 2>&1 &&
  jq -e '.spdxVersion | startswith("SPDX-")' "$sbom_tmp" >/dev/null 2>&1; then
  sbom_status="generated"
  chmod 0600 "$sbom_tmp"
else
  sbom_status="generation-failed"
  rm -f -- "$sbom_tmp"
fi

[[ "$cargo_audit_status" == 'clean' ]] ||
  die "cargo audit prerequisite is not clean: $cargo_audit_status"
[[ "$sbom_status" == 'generated' ]] ||
  die "SPDX prerequisite was not generated: $sbom_status"

jeryu_source_verify "$head_sha" "$tree_sha" "$source_inputs_sha" "$source_scope_json"

audit_sha=''
sbom_sha=''
if [[ -f "$audit_tmp" && ! -L "$audit_tmp" ]]; then
  [[ "$(stat -c '%h' "$audit_tmp")" -eq 1 ]] ||
    die 'cargo-audit evidence is multiply linked'
  audit_sha="$(sha256sum "$audit_tmp" | awk '{print $1}')"
fi
if [[ -f "$sbom_tmp" && ! -L "$sbom_tmp" ]]; then
  [[ "$(stat -c '%h' "$sbom_tmp")" -eq 1 ]] ||
    die 'SBOM evidence is multiply linked'
  sbom_sha="$(sha256sum "$sbom_tmp" | awk '{print $1}')"
fi

evidence_tmp="$(mktemp "$repo_root/target/security/.evidence.XXXXXX")"
jq -nS \
  --arg schema_version "$security_schema" \
  --arg repo 'jeryu-tool-finder' \
  --arg status 'pass' \
  --arg head "$head_sha" \
  --arg tree "$tree_sha" \
  --arg source_inputs_sha "$source_inputs_sha" \
  --argjson source_scope "$source_scope_json" \
  --arg cargo_audit "$cargo_audit_status" \
  --arg sbom "$sbom_status" \
  --arg audit_path 'target/security/cargo-audit.json' \
  --arg audit_sha "$audit_sha" \
  --arg sbom_path 'target/security/jeryu-tool-finder.spdx.json' \
  --arg sbom_sha "$sbom_sha" \
  '{
    schema_version: $schema_version,
    repo: $repo,
    status: $status,
    head: $head,
    tree: $tree,
    source: ({tracked_inputs_sha256: $source_inputs_sha} +
      (if $source_scope == null then {} else {scope:$source_scope} end)),
    checks: [
      "gitleaks-detect",
      "actionlint",
      "env-file",
      "cargo-metadata",
      "cargo-deny-locked-policy",
      "cargo-audit-no-fetch",
      "syft-sbom"
    ],
    cargo_audit: $cargo_audit,
    sbom: $sbom,
    artifacts: {
      cargo_audit: {path: $audit_path, sha256: $audit_sha},
      sbom: {path: $sbom_path, sha256: $sbom_sha}
    }
  }' > "$evidence_tmp"
chmod 0600 "$evidence_tmp"
jeryu_source_verify "$head_sha" "$tree_sha" "$source_inputs_sha" "$source_scope_json"
[[ "$(sha256sum -- "$audit_tmp" | awk '{print $1}')" == "$audit_sha" &&
   "$(sha256sum -- "$sbom_tmp" | awk '{print $1}')" == "$sbom_sha" ]] ||
  die 'security prerequisite moved before evidence publication'
mv -- "$audit_tmp" target/security/cargo-audit.json
audit_tmp=''
mv -- "$sbom_tmp" target/security/jeryu-tool-finder.spdx.json
sbom_tmp=''
mv -- "$evidence_tmp" target/security/evidence.json
evidence_tmp=''
jeryu_source_verify "$head_sha" "$tree_sha" "$source_inputs_sha" "$source_scope_json"
trap - EXIT
printf 'security ok\n'
