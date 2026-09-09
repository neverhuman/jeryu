#!/usr/bin/env bash
# Canonical fail-closed supply-chain lane used by local and protected CI.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${repo_root}"
source ops/ci/lib.sh

mkdir -p target/jankurai/security target/security
checks_tsv="target/jankurai/security/checks.tsv"
evidence_json="target/jankurai/security/evidence.json"
: >"$checks_tsv"
failed=0

record() {
  local name="$1"
  local status="$2"
  local policy="$3"
  local detail="$4"
  detail="${detail//$'\t'/ }"
  detail="${detail//$'\n'/ }"
  printf '%s\t%s\t%s\t%s\n' "$name" "$status" "$policy" "$detail" >>"$checks_tsv"
}

core_tool() {
  local name="$1"
  local path canonical mode links
  path="$(type -P -- "$name" 2>/dev/null || true)"
  [[ -n "$path" ]] || {
    printf 'security check failed: missing required core tool %s\n' "$name" >&2
    exit 1
  }
  canonical="$(realpath -e -- "$path" 2>/dev/null || true)"
  mode="$(stat -c '%a' -- "$canonical" 2>/dev/null || true)"
  links="$(stat -c '%h' -- "$canonical" 2>/dev/null || true)"
  [[ -n "$canonical" && -f "$canonical" && -x "$canonical" && "$path" == "$canonical" ]] || {
    printf 'security check failed: invalid required core tool custody for %s\n' "$name" >&2
    exit 1
  }
  [[ "$mode" =~ ^[0-7]{3,4}$ && "$links" == "1" ]] || {
    printf 'security check failed: invalid required core tool metadata for %s\n' "$name" >&2
    exit 1
  }
  if (( (8#$mode & 8#22) != 0 )); then
    printf 'security check failed: writable required core tool %s\n' "$name" >&2
    exit 1
  fi
  printf '%s\n' "$canonical"
}

sha256_bin="$(core_tool sha256sum)"
jq_bin="$(core_tool jq)"

bind_tool() {
  local output_name="$1"
  local name="$2"
  local path canonical mode links uid digest
  if (( $# == 3 )); then path=$3; else path="$(type -P -- "$name" 2>/dev/null || true)"; fi
  if [[ -z "$path" ]]; then
    record "tool:${name}" "fail" "required" "tool is not installed"
    failed=1
    printf -v "$output_name" '%s' ""
    return 1
  fi
  canonical="$(realpath -e -- "$path" 2>/dev/null || true)"
  mode="$(stat -c '%a' -- "$canonical" 2>/dev/null || true)"
  links="$(stat -c '%h' -- "$canonical" 2>/dev/null || true)"
  uid="$(stat -c '%u' -- "$canonical" 2>/dev/null || true)"
  if [[ -z "$canonical" || ! -f "$canonical" || ! -x "$canonical" || "$path" != "$canonical" ||
        ! "$mode" =~ ^[0-7]{3,4}$ || "$links" != "1" ]]; then
    record "tool:${name}" "fail" "required" "tool must be a direct executable regular file with one link"
    failed=1
    printf -v "$output_name" '%s' ""
    return 1
  fi
  if (( (8#$mode & 8#22) != 0 )); then
    record "tool:${name}" "fail" "required" "tool must not be group- or world-writable"
    failed=1
    printf -v "$output_name" '%s' ""
    return 1
  fi
  read -r digest _ < <("$sha256_bin" "$canonical")
  record "tool:${name}" "pass" "required" "path=${canonical} uid=${uid} mode=${mode} links=${links} sha256=${digest}"
  printf -v "$output_name" '%s' "$canonical"
}

# Rustup normally installs cargo as a link to its dispatcher. Resolve the
# source-pinned installed Cargo without executing that dispatcher or consulting
# RUSTUP_TOOLCHAIN. Protected broker admission remains direct-file only.
bind_cargo() {
  local output_name=$1 candidate dispatcher toolchain_root channel version
  candidate="$(type -P -- cargo 2>/dev/null || true)"
  dispatcher="$(type -P -- rustup 2>/dev/null || true)"
  if [[ ${JAIN_RELEASE_CI:-0} == 1 || -z $candidate || -z $dispatcher ||
        ! $candidate -ef $dispatcher ]]; then
    bind_tool "$output_name" cargo
    return $?
  fi
  toolchain_root=${RUSTUP_HOME:-${HOME:?HOME is required for the default Rustup installation}/.rustup}
  # The maintained toolchain must contain one numeric pin in its owning table.
  channel=''
  if [[ -f rust-toolchain.toml && ! -L rust-toolchain.toml &&
        $(stat -c %h -- rust-toolchain.toml) == 1 ]]; then
    channel=$(awk '
      /^[[:space:]]*\[/ {
        inside = ($0 ~ /^[[:space:]]*\[toolchain\][[:space:]]*$/)
        if (inside) tables++
      }
      inside && /^[[:space:]]*channel[[:space:]]*=/ {
        count++
        if ($0 !~ /^[[:space:]]*channel[[:space:]]*=[[:space:]]*"[0-9]+\.[0-9]+\.[0-9]+"[[:space:]]*$/) exit 1
        value=$0; sub(/^[^"]*"/, "", value); sub(/"[[:space:]]*$/, "", value)
      }
      END { if (tables != 1 || count != 1) exit 1; print value }
    ' rust-toolchain.toml) || channel=''
  fi
  if [[ ! $channel =~ ^[0-9]+\.[0-9]+\.[0-9]+$ || $toolchain_root != /* ||
        $(uname -s):$(uname -m) != Linux:x86_64 ]]; then
    record "tool:cargo" "fail" "required" "Rustup Cargo requires a numeric source pin, absolute installation root and supported Linux x86_64 host"
    failed=1
    printf -v "$output_name" '%s' ""
    return 1
  fi
  candidate="$toolchain_root/toolchains/$channel-x86_64-unknown-linux-gnu/bin/cargo"
  bind_tool "$output_name" cargo "$candidate" || return 1
  version=$("$candidate" --version) || version=''
  if [[ ! $version =~ ^cargo\ ([0-9]+\.[0-9]+\.[0-9]+)\ \([0-9a-f]+\ [0-9]{4}-[0-9]{2}-[0-9]{2}\)$ ||
        ${BASH_REMATCH[1]:-} != "$channel" ]]; then
    record "cargo-toolchain" "fail" "required" "resolved Cargo version does not match the source toolchain pin"
    failed=1
    printf -v "$output_name" '%s' ""
    return 1
  fi
  record "cargo-toolchain" "pass" "required" "source toolchain=$channel resolved Cargo=$candidate"
}

write_evidence() {
  "$jq_bin" -Rn '
    [inputs | split("\t") | {
      name: .[0],
      status: .[1],
      policy: .[2],
      detail: .[3]
    }] as $checks |
    {
      schema_version: "jeryu.split.security/v2",
      conclusion: (if any($checks[]; .status == "fail") then "failure" else "success" end),
      checks: $checks
    }
  ' <"$checks_tsv" >"$evidence_json"
  cp "$evidence_json" target/security/evidence.json
}

if find . -path './.git' -prune -o -path './target' -prune -o -name '.env' -type f -print -quit |
  grep -q .; then
  record "env-file" "fail" "required" "repository contains a .env file"
  failed=1
else
  record "env-file" "pass" "required" "no .env files found outside ignored build output"
fi

gitleaks_bin=""
bind_tool gitleaks_bin gitleaks || true
if [[ -n "$gitleaks_bin" ]]; then
  if {
    git ls-files -z
    git ls-files --others --exclude-standard -z
  } | sort -zu | while IFS= read -r -d '' path; do
    [[ -f "$path" ]] || continue
    case "$path" in
      target/*|.jankurai/*) continue ;;
    esac
    if LC_ALL=C grep -Iq . "$path"; then
      printf '\n===== %s =====\n' "$path"
      cat "$path"
    fi
  done | "$gitleaks_bin" detect --pipe --redact --verbose; then
    record "gitleaks-detect" "pass" "required" "tracked and untracked text passed secret scanning"
  else
    record "gitleaks-detect" "fail" "required" "secret scanner returned nonzero"
    failed=1
  fi
fi

mapfile -d '' -t workflow_files < <(
  find .github/workflows -maxdepth 1 -type f \( -name '*.yml' -o -name '*.yaml' \) -print0 2>/dev/null || true
)
if [[ "${#workflow_files[@]}" -gt 0 ]]; then
  actionlint_bin=""
  bind_tool actionlint_bin actionlint || true
  if [[ -n "$actionlint_bin" ]]; then
    if "$actionlint_bin" "${workflow_files[@]}"; then
      record "actionlint" "pass" "required" "hosted workflow syntax and semantics passed"
    else
      record "actionlint" "fail" "required" "workflow linter returned nonzero"
      failed=1
    fi
  fi
else
  record "actionlint" "not_applicable" "no-workflows" "repository has no hosted workflow files"
fi

if [[ -f Cargo.toml ]]; then
  cargo_bin=""
  bind_cargo cargo_bin || true
  if [[ -n "$cargo_bin" ]]; then
    if "$cargo_bin" metadata --locked --format-version 1 --no-deps >/dev/null; then
      record "cargo-metadata" "pass" "required" "workspace dependency metadata resolves"
    else
      record "cargo-metadata" "fail" "required" "cargo metadata returned nonzero"
      failed=1
    fi
  fi
else
  record "cargo-metadata" "not_applicable" "no-manifest" "portal has no Rust workspace manifest"
fi

if [[ -f Cargo.lock ]]; then
  cargo_audit_bin=""
  bind_tool cargo_audit_bin cargo-audit || true
  if [[ -n "$cargo_audit_bin" ]]; then
    if "$cargo_audit_bin" audit --no-fetch --format json >target/security/cargo-audit.json; then
      record "cargo-audit-no-fetch" "pass" "required" "locked dependencies passed the cached advisory database"
    else
      record "cargo-audit-no-fetch" "fail" "required" "advisory findings or unavailable cached database"
      failed=1
    fi
  fi
else
  record "cargo-audit-no-fetch" "not_applicable" "no-lock" "portal has no Cargo lockfile"
fi

syft_bin=""
bind_tool syft_bin syft || true
if [[ -n "$syft_bin" ]]; then
  sbom="target/security/jeryu.cdx.json"
  source_version="$(<VERSION)"
  [[ "$source_version" =~ ^[a-z0-9][a-z0-9._-]{0,127}$ ]] || {
    record "syft-sbom" "fail" "required" "VERSION is not a bounded immutable-tag identity"
    failed=1
    source_version="invalid"
  }
  if "$syft_bin" dir:. --exclude './target/**' --exclude './.git/**' \
    --source-name jeryu --source-version "$source_version" \
    --output "cyclonedx-json=${sbom}" >/dev/null &&
    "$jq_bin" -e '.bomFormat == "CycloneDX" and (.specVersion | type == "string")' "$sbom" >/dev/null; then
    record "syft-sbom" "pass" "required" "CycloneDX source inventory generated and parsed"
  else
    record "syft-sbom" "fail" "required" "SBOM generation or validation returned nonzero"
    failed=1
  fi
fi

write_evidence
if [[ "$failed" -ne 0 ]]; then
  printf 'security check failed; evidence written to %s\n' "$evidence_json" >&2
  exit 1
fi
printf 'security ok; evidence written to %s\n' "$evidence_json"
