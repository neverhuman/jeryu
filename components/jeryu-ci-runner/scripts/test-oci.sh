#!/usr/bin/env bash
# Required real engine proof; run only inside a disposable Linux host.
set -euo pipefail
umask 077
component=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
cd "$component"
[[ ${JERYU_DISPOSABLE_SANDBOX:-0} == 1 && $(id -u) == 0 ]] || {
  printf 'OCI proof requires root and JERYU_DISPOSABLE_SANDBOX=1 in a disposable Linux host\n' >&2
  exit 1
}
# Set to 1 only when the exact pinned base is already loaded in this guest daemon.
# Unset or 0 retains the normal pull; preloaded mode never pulls on failure.
case ${JERYU_OCI_PRELOADED_BASE-0} in
  0|1) ;;
  *) printf 'JERYU_OCI_PRELOADED_BASE must be 0 or 1\n' >&2; exit 1 ;;
esac
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
manifest=crates/jeryu-runner-oci/Cargo.toml
target=$(cargo metadata --locked --no-deps --format-version 1 --manifest-path "$manifest" | jq -er .target_directory)
mkdir -p "$target/ci"
build_log=$(mktemp "$target/ci/oci-build.XXXXXXXX.jsonl")
cargo build --locked --manifest-path "$manifest" --example oci_probe --message-format=json > "$build_log"
JERYU_OCI_PROBE_BIN=$(jq -ers '[.[] | select(.reason == "compiler-artifact" and .target.name == "oci_probe" and .executable != null) | .executable] | if length == 1 then .[0] else error("expected exactly one probe artifact") end' "$build_log")
export JERYU_OCI_PROBE_BIN
JERYU_OCI_OUTPUT_DIR=$(mktemp -d "$target/ci/oci.XXXXXXXX")
export JERYU_OCI_OUTPUT_DIR
cargo test --locked --manifest-path "$manifest" --test real_docker_smoke -- \
  --include-ignored --exact hardened_oci_profile_enforces_required_matrix --nocapture --test-threads=1 \
  2>&1 | tee "$target/ci/oci.log"
jq -e '.schema_version == "jeryu.oci-proof/v1" and .failures == 0 and .skipped == 0 and
  .checks == ["workspace-root", "host-sockets", "credential-env", "network-egress", "pids", "memory",
    "seccomp", "no-new-privileges", "cgroup-limits"]' "$JERYU_OCI_OUTPUT_DIR/receipt.json" >/dev/null
printf 'Verified OCI receipt: %s/receipt.json\n' "$JERYU_OCI_OUTPUT_DIR"
