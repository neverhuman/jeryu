#!/usr/bin/env bash
# Resolve GitHub release workflow inputs into version/dry_run outputs.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$SCRIPT_DIR/lib.sh"

event_name="${GITHUB_EVENT_NAME:-${EVENT_NAME:-}}"
ref_name="${GITHUB_REF_NAME:-}"
input_version="${INPUT_VERSION:-}"
input_dry_run="${INPUT_DRY_RUN:-false}"

case "$event_name" in
  push)
    if [ -z "$ref_name" ]; then
      die "GITHUB_REF_NAME is required for tag push releases"
    fi
    version="${ref_name#v}"
    dry_run="false"
    ;;
  workflow_dispatch)
    if [ -z "$input_version" ]; then
      die "workflow_dispatch release requires INPUT_VERSION"
    fi
    version="$input_version"
    dry_run="${input_dry_run:-false}"
    ;;
  *)
    die "unsupported release event: ${event_name:-unset}"
    ;;
esac

if [ -z "${GITHUB_OUTPUT:-}" ]; then
  printf 'version=%s\n' "$version"
  printf 'dry_run=%s\n' "$dry_run"
else
  printf 'version=%s\n' "$version" >> "$GITHUB_OUTPUT"
  printf 'dry_run=%s\n' "$dry_run" >> "$GITHUB_OUTPUT"
fi
