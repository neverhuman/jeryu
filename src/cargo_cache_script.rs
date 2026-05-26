//! Owner: Cargo cache — CI pre-build script renderer
//! Proof: `cargo test -p jeryu -- cargo_cache`
//! Invariants:
//!   - The script is idempotent: running it twice does not corrupt state.
//!   - Lease files are always removed via EXIT trap, even on job cancellation.
//!   - sccache installation is attempted only when the binary is absent.

use crate::cargo_cache::{
    CACHE_HOME_DIR_NAME, CACHE_PROMOTION_MARKERS_DIR, CACHE_SEED_MARKERS_DIR, CACHE_STAMP_FILE,
    LEASES_DIR_NAME, RUSTUP_HOME_DIR_NAME,
};
use crate::cargo_cache::helpers::shell_quote;

/// Render the CI pre-build shell script that configures the Cargo/sccache
/// environment inside a runner container before each job.
///
/// The script:
/// 1. Detects the active Rust toolchain and computes a stable cache key.
/// 2. Points `CARGO_HOME`, `RUSTUP_HOME`, and `CARGO_TARGET_DIR` at the
///    shared pool cache mount.
/// 3. Optionally installs sccache from GitHub releases if missing.
/// 4. Writes a lease file so the cache GC never reclaims live target dirs.
///
/// `pool_cache_mount` — the path where the pool cache volume is mounted
/// inside the container (e.g. `/pool-cache`).
pub fn render_runner_cargo_pre_build_script(pool_cache_mount: &str, executor: &str) -> String {
    let _ = executor;
    let pool_cache_mount = shell_quote(pool_cache_mount);
    let sccache_version = shell_quote(&crate::settings::get().sccache.binary_version);
    format!(
        r#"set -eu
JERYU_CARGO_PREREQS_OK=1
for tool in awk cat cut date mkdir rm rmdir sha256sum; do
  command -v "$tool" >/dev/null 2>&1 || JERYU_CARGO_PREREQS_OK=0
done
if [ "${{JERYU_CARGO_CACHE:-1}}" != "0" ] && [ "$JERYU_CARGO_PREREQS_OK" = "1" ] && command -v cargo >/dev/null 2>&1 && command -v rustc >/dev/null 2>&1; then
  JERYU_CARGO_CACHE_ROOT={pool_cache_mount}
  JERYU_CARGO_CARGO_HOME="$JERYU_CARGO_CACHE_ROOT/{cargo_home_dir}"
  JERYU_CARGO_RUSTUP_HOME="$JERYU_CARGO_CACHE_ROOT/{rustup_home_dir}"
  mkdir -p "$JERYU_CARGO_CARGO_HOME" "$JERYU_CARGO_RUSTUP_HOME"
  export CARGO_HOME="$JERYU_CARGO_CARGO_HOME"
  export RUSTUP_HOME="$JERYU_CARGO_RUSTUP_HOME"
  if [ "${{JERYU_SCCACHE_ENABLED:-1}}" != "0" ]; then
    export SCCACHE_DIR="$JERYU_CARGO_CACHE_ROOT/sccache"
    export SCCACHE_NO_DAEMON=1
    if [ -n "${{JERYU_SCCACHE_CACHE_SIZE:-}}" ]; then
      export SCCACHE_CACHE_SIZE="$JERYU_SCCACHE_CACHE_SIZE"
    fi
    mkdir -p "$SCCACHE_DIR"
  fi
  RUSTC_INFO="$(rustc -vV)"
  HOST_TRIPLE="$(printf '%s\n' "$RUSTC_INFO" | awk '/^host: / {{ print $2; exit }}')"
  RUSTC_VERSION="$(printf '%s\n' "$RUSTC_INFO" | awk '/^release: / {{ print $2; exit }}')"
  if [ -n "$HOST_TRIPLE" ] && [ -n "$RUSTC_VERSION" ]; then
    RUSTC_KEY="$(printf '%s\n' "$RUSTC_INFO" | sha256sum | cut -c1-12)"
    JERYU_CARGO_SCOPE_KEY="${{CI_PROJECT_PATH_SLUG:-unknown-project}}"
    JERYU_SCCACHE_VERSION={sccache_version}
    JERYU_CARGO_TARGET_PROFILE="${{JERYU_CARGO_TARGET_PROFILE:-debug}}"
    JERYU_CARGO_TARGET_ROOT="$JERYU_CARGO_CACHE_ROOT/cargo-targets/$JERYU_CARGO_SCOPE_KEY/$RUSTC_KEY/$HOST_TRIPLE"
    JERYU_CARGO_SHARED_TARGET_ROOT="$JERYU_CARGO_CACHE_ROOT/cargo-targets/$JERYU_CARGO_SCOPE_KEY/$RUSTC_KEY/$HOST_TRIPLE"
    case "${{JERYU_CARGO_TARGET_ISOLATE:-slot}}" in
      shared|none)
        JERYU_CARGO_TARGET_ROLE="shared"
        ;;
      id)
        JERYU_CARGO_TARGET_ROOT="$JERYU_CARGO_TARGET_ROOT/job-ids/${{CI_JOB_ID:-unknown}}"
        JERYU_CARGO_TARGET_ROLE="job"
        ;;
      slot|concurrent)
        JERYU_CARGO_MANAGER_KEY="${{CI_BUILDS_DIR:-}}"
        JERYU_CARGO_MANAGER_KEY="${{JERYU_CARGO_MANAGER_KEY##*/}}"
        if [ -z "$JERYU_CARGO_MANAGER_KEY" ]; then
          JERYU_CARGO_MANAGER_KEY="${{CI_RUNNER_SHORT_TOKEN:-${{CI_RUNNER_ID:-runner}}}}"
        fi
        JERYU_CARGO_SLOT_KEY="$JERYU_CARGO_MANAGER_KEY-${{CI_CONCURRENT_ID:-${{CI_CONCURRENT_PROJECT_ID:-0}}}}"
        JERYU_CARGO_TARGET_ROOT="$JERYU_CARGO_TARGET_ROOT/slots/$JERYU_CARGO_SLOT_KEY"
        JERYU_CARGO_TARGET_ROLE="slot"
        ;;
      job|name|*)
        JERYU_CARGO_TARGET_ROOT="$JERYU_CARGO_TARGET_ROOT/jobs/${{CI_JOB_NAME_SLUG:-unknown}}"
        JERYU_CARGO_TARGET_ROLE="job"
        ;;
    esac
    export JERYU_CARGO_CACHE_ROOT JERYU_CARGO_SCOPE_KEY JERYU_CARGO_RUSTC_KEY="$RUSTC_KEY" JERYU_CARGO_RUSTC_VERSION="$RUSTC_VERSION" JERYU_CARGO_HOST_TRIPLE="$HOST_TRIPLE"
    export CARGO_TARGET_DIR="$JERYU_CARGO_TARGET_ROOT/target"
    mkdir -p "$CARGO_TARGET_DIR"
    JERYU_CARGO_TARGET_STAMP="$CARGO_TARGET_DIR/{cache_stamp_file}"
    JERYU_CARGO_SHARED_TARGET_STAMP="$JERYU_CARGO_SHARED_TARGET_ROOT/target/{cache_stamp_file}"
    JERYU_CARGO_TARGET_SEEDS="$CARGO_TARGET_DIR/{cache_seed_markers_dir}"
    JERYU_CARGO_TARGET_PROMOTIONS="$CARGO_TARGET_DIR/{cache_promotion_markers_dir}"
    mkdir -p "$CARGO_TARGET_DIR" "$JERYU_CARGO_TARGET_SEEDS" "$JERYU_CARGO_TARGET_PROMOTIONS"
    cat > "$JERYU_CARGO_TARGET_STAMP" <<EOF
{{"schema_version":"1","scope_key":"$JERYU_CARGO_SCOPE_KEY","rustc_key":"$JERYU_CARGO_RUSTC_KEY","host_triple":"$JERYU_CARGO_HOST_TRIPLE","target_profile":"$JERYU_CARGO_TARGET_PROFILE","target_role":"$JERYU_CARGO_TARGET_ROLE"}}
EOF
    if [ -f "$JERYU_CARGO_SHARED_TARGET_STAMP" ] && [ "${{JERYU_CARGO_TARGET_ROLE:-}}" = "slot" ]; then
      if cmp -s "$JERYU_CARGO_TARGET_STAMP" "$JERYU_CARGO_SHARED_TARGET_STAMP"; then
        if [ -z "$(find "$CARGO_TARGET_DIR" -mindepth 1 -maxdepth 1 ! -name '{cache_stamp_file}' ! -name '{cache_seed_markers_dir}' ! -name '{cache_promotion_markers_dir}' -print -quit)" ]; then
          cp -a "$JERYU_CARGO_SHARED_TARGET_ROOT/target/." "$CARGO_TARGET_DIR/"
          cat > "$JERYU_CARGO_TARGET_SEEDS/${{CI_JOB_ID:-job}}-$$.json" <<EOF
{{"seeded_at":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","scope_key":"$JERYU_CARGO_SCOPE_KEY","rustc_key":"$JERYU_CARGO_RUSTC_KEY","host_triple":"$JERYU_CARGO_HOST_TRIPLE","target_profile":"$JERYU_CARGO_TARGET_PROFILE"}}
EOF
          mkdir -p "$JERYU_CARGO_SHARED_TARGET_ROOT/target/{cache_promotion_markers_dir}"
          cat > "$JERYU_CARGO_SHARED_TARGET_ROOT/target/{cache_promotion_markers_dir}/${{CI_JOB_ID:-job}}-$$.json" <<EOF
{{"promoted_at":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","scope_key":"$JERYU_CARGO_SCOPE_KEY","rustc_key":"$JERYU_CARGO_RUSTC_KEY","host_triple":"$JERYU_CARGO_HOST_TRIPLE","target_profile":"$JERYU_CARGO_TARGET_PROFILE"}}
EOF
        fi
      fi
    fi
    JERYU_CARGO_LEASE_DIR="$CARGO_TARGET_DIR/{leases_dir}"
    mkdir -p "$JERYU_CARGO_LEASE_DIR"
    JERYU_CARGO_LEASE_FILE="$JERYU_CARGO_LEASE_DIR/${{CI_JOB_ID:-job}}-$$.json"
    cat > "$JERYU_CARGO_LEASE_FILE" <<EOF
{{"kind":"runner-cargo","scope_key":"$JERYU_CARGO_SCOPE_KEY","target_dir":"$CARGO_TARGET_DIR","pid":$$,"created_at":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","rustc_key":"$JERYU_CARGO_RUSTC_KEY","rustc_version":"$JERYU_CARGO_RUSTC_VERSION","host_triple":"$JERYU_CARGO_HOST_TRIPLE"}}
EOF
    trap 'rm -f "$JERYU_CARGO_LEASE_FILE"; rmdir "$JERYU_CARGO_LEASE_DIR" 2>/dev/null || true' EXIT
    if [ -n "${{JERYU_CARGO_INCREMENTAL:-}}" ]; then
      export CARGO_INCREMENTAL="$JERYU_CARGO_INCREMENTAL"
    else
      export CARGO_INCREMENTAL=0
    fi
    if [ "${{JERYU_CARGO_AUTOSIZE:-1}}" != "0" ]; then
      if [ -z "${{JERYU_CARGO_HOST_CORES:-}}" ]; then
        if command -v getconf >/dev/null 2>&1; then
          JERYU_CARGO_HOST_CORES="$(getconf _NPROCESSORS_ONLN 2>/dev/null || true)"
        elif command -v nproc >/dev/null 2>&1; then
          JERYU_CARGO_HOST_CORES="$(nproc 2>/dev/null || true)"
        fi
      fi
      case "${{JERYU_CARGO_HOST_CORES:-}}" in
        ''|*[!0-9]*) JERYU_CARGO_HOST_CORES=4 ;;
      esac
      if [ -z "${{JERYU_CARGO_RESERVED_CORES:-}}" ]; then
        JERYU_CARGO_RESERVED_CORES=$((JERYU_CARGO_HOST_CORES / 4))
        if [ "$JERYU_CARGO_RESERVED_CORES" -lt 32 ]; then
          JERYU_CARGO_RESERVED_CORES=32
        fi
        JERYU_CARGO_HALF_CORES=$((JERYU_CARGO_HOST_CORES / 2))
        if [ "$JERYU_CARGO_RESERVED_CORES" -gt "$JERYU_CARGO_HALF_CORES" ]; then
          JERYU_CARGO_RESERVED_CORES="$JERYU_CARGO_HALF_CORES"
        fi
      fi
      case "$JERYU_CARGO_RESERVED_CORES" in
        ''|*[!0-9]*) JERYU_CARGO_RESERVED_CORES=1 ;;
      esac
      if [ "$JERYU_CARGO_RESERVED_CORES" -ge "$JERYU_CARGO_HOST_CORES" ]; then
        JERYU_CARGO_RESERVED_CORES=$((JERYU_CARGO_HOST_CORES - 1))
      fi
      if [ "$JERYU_CARGO_RESERVED_CORES" -lt 1 ]; then
        JERYU_CARGO_RESERVED_CORES=1
      fi
      JERYU_CARGO_TOTAL_SLOTS="${{JERYU_CARGO_TOTAL_RUNNER_SLOTS:-${{JERYU_RUNNER_FLEET_TOTAL_SLOTS:-${{JERYU_RUNNER_POOL_TOTAL_SLOTS:-${{JERYU_CARGO_RUNNER_SLOTS:-20}}}}}}}}"
      case "$JERYU_CARGO_TOTAL_SLOTS" in
        ''|*[!0-9]*) JERYU_CARGO_TOTAL_SLOTS=20 ;;
      esac
      if [ "$JERYU_CARGO_TOTAL_SLOTS" -lt 1 ]; then
        JERYU_CARGO_TOTAL_SLOTS=1
      fi
      JERYU_CARGO_USABLE_CORES=$((JERYU_CARGO_HOST_CORES - JERYU_CARGO_RESERVED_CORES))
      if [ "$JERYU_CARGO_USABLE_CORES" -lt 1 ]; then
        JERYU_CARGO_USABLE_CORES=1
      fi
      JERYU_CARGO_AUTO_BUILD_JOBS=$((JERYU_CARGO_USABLE_CORES / JERYU_CARGO_TOTAL_SLOTS))
      if [ "$JERYU_CARGO_AUTO_BUILD_JOBS" -lt "${{JERYU_CARGO_MIN_BUILD_JOBS:-1}}" ]; then
        JERYU_CARGO_AUTO_BUILD_JOBS="${{JERYU_CARGO_MIN_BUILD_JOBS:-1}}"
      fi
      if [ "$JERYU_CARGO_AUTO_BUILD_JOBS" -gt "${{JERYU_CARGO_MAX_BUILD_JOBS:-16}}" ]; then
        JERYU_CARGO_AUTO_BUILD_JOBS="${{JERYU_CARGO_MAX_BUILD_JOBS:-16}}"
      fi
      case "${{CARGO_BUILD_JOBS:-}}" in
        ''|*[!0-9]*)
          export CARGO_BUILD_JOBS="$JERYU_CARGO_AUTO_BUILD_JOBS"
          ;;
        *)
          if [ "$CARGO_BUILD_JOBS" -gt "$JERYU_CARGO_AUTO_BUILD_JOBS" ]; then
            export CARGO_BUILD_JOBS="$JERYU_CARGO_AUTO_BUILD_JOBS"
          fi
          ;;
      esac
      export JERYU_CARGO_HOST_CORES JERYU_CARGO_RESERVED_CORES JERYU_CARGO_TOTAL_SLOTS JERYU_CARGO_AUTO_BUILD_JOBS
    fi
    if [ "${{JERYU_SCCACHE_ENABLED:-1}}" != "0" ] && ! command -v sccache >/dev/null 2>&1; then
      JERYU_TOOLS_DIR="$JERYU_CARGO_CACHE_ROOT/tools"
      JERYU_SCCACHE_BIN="$JERYU_TOOLS_DIR/sccache"
      if [ ! -x "$JERYU_SCCACHE_BIN" ] && command -v curl >/dev/null 2>&1 && command -v tar >/dev/null 2>&1; then
        mkdir -p "$JERYU_TOOLS_DIR/.tmp"
        JERYU_SCCACHE_TMP="$JERYU_TOOLS_DIR/.tmp/sccache-$JERYU_SCCACHE_VERSION-$$.tar.gz"
        JERYU_SCCACHE_EXTRACT="$JERYU_TOOLS_DIR/.tmp/sccache-$JERYU_SCCACHE_VERSION-$$"
        rm -rf "$JERYU_SCCACHE_EXTRACT"
        mkdir -p "$JERYU_SCCACHE_EXTRACT"
        if curl -fsSL "https://github.com/mozilla/sccache/releases/download/$JERYU_SCCACHE_VERSION/sccache-$JERYU_SCCACHE_VERSION-x86_64-unknown-linux-musl.tar.gz" -o "$JERYU_SCCACHE_TMP"; then
          if tar -xzf "$JERYU_SCCACHE_TMP" -C "$JERYU_SCCACHE_EXTRACT" "sccache-$JERYU_SCCACHE_VERSION-x86_64-unknown-linux-musl/sccache" 2>/dev/null; then
            if mv "$JERYU_SCCACHE_EXTRACT/sccache-$JERYU_SCCACHE_VERSION-x86_64-unknown-linux-musl/sccache" "$JERYU_SCCACHE_BIN" && chmod 0755 "$JERYU_SCCACHE_BIN"; then
              :
            else
              rm -f "$JERYU_SCCACHE_BIN"
            fi
          fi
        fi
        rm -f "$JERYU_SCCACHE_TMP"
        rm -rf "$JERYU_SCCACHE_EXTRACT"
      fi
      if [ -x "$JERYU_SCCACHE_BIN" ]; then
        export PATH="$JERYU_TOOLS_DIR:$PATH"
      fi
    fi
    if [ "${{JERYU_SCCACHE_ENABLED:-1}}" != "0" ] && command -v sccache >/dev/null 2>&1; then
      export RUSTC_WRAPPER=sccache
    fi
  fi
fi
"#,
        leases_dir = LEASES_DIR_NAME,
        cache_stamp_file = CACHE_STAMP_FILE,
        cache_seed_markers_dir = CACHE_SEED_MARKERS_DIR,
        cache_promotion_markers_dir = CACHE_PROMOTION_MARKERS_DIR,
        cargo_home_dir = CACHE_HOME_DIR_NAME,
        rustup_home_dir = RUSTUP_HOME_DIR_NAME,
        pool_cache_mount = pool_cache_mount,
        sccache_version = sccache_version,
    )
}
