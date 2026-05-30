#!/usr/bin/env bash
# Shared CI helper library for local lanes and hosted mirrors.
set -euo pipefail

source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/common.sh"

ci_log() {
  printf '[jeryu-ci] %s\n' "$*"
}
