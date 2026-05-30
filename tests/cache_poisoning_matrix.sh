#!/usr/bin/env bash
set -euo pipefail
cargo test -p jeryu-cache-core policy
cargo test -p jeryu-cache-core cache_key
cargo test -p jeryu-cache-adversary adversarial_cache_laws_block_known_attacks

tmp="${TMPDIR:-/tmp}/jeryu-cache-poisoning-matrix-$$"
log="${tmp}/self-test.log"
root="${tmp}/cache-root"
mkdir -p "${tmp}"
trap 'rm -rf "${tmp}"' EXIT

cargo run -p jeryu-cache -- self-test "${root}" | tee "${log}"

grep -Fq "ok: fork PR cannot write trusted cache" "${log}"
grep -Fq "ok: cross-project read denied by default" "${log}"
grep -Fq "ok: release ignores mutable cache" "${log}"
grep -Fq "ok: cache service outage safe-miss" "${log}"
grep -Fq "ok: false-hit detector" "${log}"
grep -Fq "phase6 adversarial suite: ok (7 scenarios)" "${log}"
