#!/usr/bin/env bash
# Deterministic fast lane: the pin must always match the manifest.
set -euo pipefail
source ops/ci/lib.sh
bash ops/ci/check-rendered-identity.sh --check --repo jeryu-tool
printf 'fast ok: jeryu-tool consumers match tool-manifest.toml\n'
