#!/usr/bin/env bash
set -euo pipefail

# BEGIN GENERATED JANKURAI PIN — DO NOT EDIT
export JERYU_JANKURAI_SOURCE_REPO="http://127.0.0.1:8787/git/jeryu/jankurai.git"
export JERYU_JANKURAI_VERSION="jankurai 1.6.11"
export JERYU_JANKURAI_SHA256="9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c"
export JERYU_JANKURAI_SOURCE_REV="b88562fdb124aa86dedd70ab972e7d0d87e58be1"
export JERYU_JANKURAI_SOURCE_TAG="v1.6.11-deadlang-precision-split.3"
export JERYU_JANKURAI_SOURCE_TREE="611229e54938c0e8808896e369fd54d095d258f7"
export JERYU_JANKURAI_SOURCE_ARCHIVE_SHA256="903a231eca8f6a1f050953b603d5a278a1606abcdf47434eb1b45262d74068aa"
export JERYU_JANKURAI_CARGO_LOCK_SHA256="b9acb981c326226a687d0b6703e4f7ee303148e9e1a6dda1aa03d77988820f6a"
export JERYU_JANKURAI_RUST_TOOLCHAIN="1.95.0"
export JERYU_JANKURAI_RUSTC_VERSION="rustc 1.95.0 (59807616e 2026-04-14)"
export JERYU_JANKURAI_CARGO_VERSION="cargo 1.95.0 (f2d3ce0bd 2026-03-21)"
export JERYU_JANKURAI_TARGET_TRIPLE="x86_64-unknown-linux-gnu"
export JERYU_JANKURAI_BUILD_MODE="oci-vendor-locked-offline-workspace-member-v2"
export JERYU_JANKURAI_PACKAGE_PATH="crates/jankurai"
export JERYU_JANKURAI_BUILDER_IMAGE="rust@sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082"
export JERYU_JANKURAI_BUILDER_IMAGE_ID="sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082"
export JERYU_JANKURAI_LINKER_VERSION="GNU ld (GNU Binutils for Debian) 2.40"
export JERYU_JANKURAI_GLIBC_VERSION="ldd (Debian GLIBC 2.36-9+deb12u14) 2.36"
export JERYU_JANKURAI_VENDOR_FILES_SHA256="a7e332f4495d9748ea020ae8ee37c4240f0f035059799bd3dc74497437143d99"
export JERYU_JANKURAI_VENDOR_FILE_COUNT="14889"
export JERYU_JANKURAI_CARGO_CONFIG_SHA256="b8982c761d62e447f2d1653c199d2d58e6b2de6c5a6f8ddba3d38e47b7f863d6"
export JERYU_JANKURAI_BUILD_ENVIRONMENT="CARGO_NET_OFFLINE=true,HOME=/tmp,LANG=C,LC_ALL=C,SOURCE_DATE_EPOCH=0,TZ=UTC"
export JERYU_JANKURAI_RUSTFLAGS="--remap-path-prefix=/opt/jeryu/jankurai=/jankurai-build/source --remap-path-prefix=/opt/jeryu/vendor=/jankurai-build/vendor --remap-path-prefix=/opt/jeryu/target=/jankurai-build/target --remap-path-prefix=/usr/local/cargo=/jankurai-build/cargo"
export JERYU_JANKURAI_BUILD_COMMAND="cargo install --locked --offline --path /opt/jeryu/jankurai/crates/jankurai --root /opt/jeryu/out --bin jankurai"
export JERYU_JANKURAI_BUILD_CONTEXT_SHA256="889d19f86fc390b0f0cf0bd6ecb4d451c51a2d6fb328e5520e4310e7ee5dedd6"
# END GENERATED JANKURAI PIN
source ops/ci/lib.sh
cd "$REPO_ROOT"
require_jankurai
for tool in cargo-llvm-cov jq; do
  require_tool "$tool"
done

[[ -z "$(git status --porcelain=v1 --untracked-files=all)" ]] || {
  printf 'coverage requires a clean committed source tree\n' >&2
  exit 1
}
for path in target target/llvm-cov target/jankurai target/jankurai/coverage; do
  if [[ -L "$path" || ( -e "$path" && ! -d "$path" ) ]]; then
    printf 'coverage output is not a physical directory: %s\n' "$path" >&2
    exit 1
  fi
  [[ -d "$path" ]] || mkdir -m 0750 -- "$path"
  [[ "$(realpath -e -- "$path")" == "$REPO_ROOT/$path" ]] || {
    printf 'coverage output is aliased: %s\n' "$path" >&2
    exit 1
  }
done

version="$(cargo llvm-cov --version)"
[[ "$version" == 'cargo-llvm-cov 0.8.7' ]] || {
  printf 'expected cargo-llvm-cov 0.8.7, got %s\n' "$version" >&2
  exit 1
}
rm -f -- target/llvm-cov/lcov.info target/jankurai/coverage/rust-lcov.info
cargo llvm-cov --locked --offline -p jeryu-tool-control --all-targets \
  --lcov --output-path target/llvm-cov/lcov.info
[[ -s target/llvm-cov/lcov.info ]]
install -m 0444 -- target/llvm-cov/lcov.info target/jankurai/coverage/rust-lcov.info
jankurai coverage audit . --config agent/coverage-sources.toml \
  --json target/jankurai/coverage/coverage-audit.json \
  --md target/jankurai/coverage/coverage-audit.md
jq -e '.summary.status == "pass" and .summary.hard_findings == 0' \
  target/jankurai/coverage/coverage-audit.json >/dev/null
printf 'coverage ok: jeryu-tool-control\n'
