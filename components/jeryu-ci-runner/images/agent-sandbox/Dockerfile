# Minimal, locked-down sandbox image for a confined coding agent.
#
# Pairs with `OciSpec::from_agent_job` (crates/jeryu-runner-oci): that spec runs this
# image with a read-only root, all capabilities dropped, no-new-privileges, the
# seccomp profile below, a non-root user, memory/pid caps, `--network none`, and ONLY
# the agent's workspace bind-mounted at /workspace. So the image is the ONLY place the
# toolchain lives — there are no host mounts. It therefore carries exactly what a
# coding agent needs to build this repo and nothing else:
#   * the Rust toolchain (pinned), * Node + the Vite/TS/React toolchain,
#   * this repo's prefetched build dependencies (cargo + npm caches),
#   * the agent CLIs: Codex, Jekko, Claude,
#   * the pinned jankurai auditor (the in-sandbox CI lanes audit offline),
#   * the jeryu-git guard installed AS `git` (deny-by-default allowlist) + refusal wrappers
#     for gh/curl/wget/ssh/scp on a controlled PATH,
#   * a non-root user `agent` (uid/gid 1000).
# Credentials are NEVER baked in — they are injected per-run (see jeryu-agent-auth).
#
# Build:  docker build -f images/agent-sandbox/Dockerfile -t localhost/jeryu/agent-sandbox:latest .
#         (the runner references it via JERYU_AGENT_IMAGE)

# ---- toolchain build stage: compile the jeryu-git guard from the repo ----------------
# The guard lives in a Cargo WORKSPACE whose root Cargo.toml lists every member, so a
# manifest-path build of one crate still makes cargo read all members. We therefore copy
# the whole (already-small, see .dockerignore) context and build the one binary by
# package selector. The guard is pure std, so this is fast and self-contained.
FROM rust:1.95-slim-bookworm AS guard-build
ENV CARGO_HOME=/opt/rust/cargo
WORKDIR /src
COPY . /src
RUN cargo build --release -p jeryu-git-guard --bin jeryu-git \
    && cp target/release/jeryu-git /jeryu-git
# Warm this repo's cargo registry cache here, where the FULL workspace source exists, so
# the runtime image can ship it for an offline (`--network none`) first build. Doing it
# in this stage avoids copying every member crate into the runtime just to satisfy
# `cargo fetch`'s workspace-manifest read.
RUN cargo fetch --locked || cargo fetch || true

# ---- jankurai build stage: compile the pinned auditor ---------------------------------
# KEEP IN SYNC with ops/ci/ensure-jankurai.sh (the pin source of truth). The runtime
# container runs `--network none`, so the auditor must ship IN the image — a CI lane
# inside the sandbox can never cargo-install it at session time. Bumping the pin means
# bumping it there first, then rebuilding this image.
FROM rust:1.95-slim-bookworm AS jankurai-build
ARG JANKURAI_REPO="http://127.0.0.1:8787/git/jeryu/jankurai.git"
ARG JANKURAI_REV="b88562fdb124aa86dedd70ab972e7d0d87e58be1"
ARG JANKURAI_VERSION="jankurai 1.6.11"
ARG JANKURAI_BUILD_CONTEXT_SHA256="889d19f86fc390b0f0cf0bd6ecb4d451c51a2d6fb328e5520e4310e7ee5dedd6"
ARG JANKURAI_BUILD_COMMAND="cargo install --locked --offline --path /opt/jeryu/jankurai/crates/jankurai --root /opt/jeryu/out --bin jankurai"
ARG JANKURAI_RUSTFLAGS="--remap-path-prefix=/opt/jeryu/jankurai=/jankurai-build/source --remap-path-prefix=/opt/jeryu/vendor=/jankurai-build/vendor --remap-path-prefix=/opt/jeryu/target=/jankurai-build/target --remap-path-prefix=/usr/local/cargo=/jankurai-build/cargo"
ARG JANKURAI_BUILD_ENVIRONMENT="CARGO_NET_OFFLINE=true,HOME=/tmp,LANG=C,LC_ALL=C,SOURCE_DATE_EPOCH=0,TZ=UTC"
ARG JANKURAI_CARGO_CONFIG_SHA256="b8982c761d62e447f2d1653c199d2d58e6b2de6c5a6f8ddba3d38e47b7f863d6"
ARG JANKURAI_VENDOR_FILE_COUNT="14889"
ARG JANKURAI_VENDOR_FILES_SHA256="a7e332f4495d9748ea020ae8ee37c4240f0f035059799bd3dc74497437143d99"
ARG JANKURAI_GLIBC_VERSION="ldd (Debian GLIBC 2.36-9+deb12u14) 2.36"
ARG JANKURAI_LINKER_VERSION="GNU ld (GNU Binutils for Debian) 2.40"
ARG JANKURAI_BUILDER_IMAGE_ID="sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082"
ARG JANKURAI_BUILDER_IMAGE="rust@sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082"
ARG JANKURAI_PACKAGE_PATH="crates/jankurai"
ARG JANKURAI_BUILD_MODE="oci-vendor-locked-offline-workspace-member-v2"
ARG JANKURAI_TARGET_TRIPLE="x86_64-unknown-linux-gnu"
ARG JANKURAI_CARGO_VERSION="cargo 1.95.0 (f2d3ce0bd 2026-03-21)"
ARG JANKURAI_RUSTC_VERSION="rustc 1.95.0 (59807616e 2026-04-14)"
ARG JANKURAI_RUST_TOOLCHAIN="1.95.0"
ARG JANKURAI_BINARY_SHA256="9e6b8857a26f6004d4c74e510e13b06d880f2e2ae0c89502698889ed690c5d6c"
ARG JANKURAI_CARGO_LOCK_SHA256="b9acb981c326226a687d0b6703e4f7ee303148e9e1a6dda1aa03d77988820f6a"
ARG JANKURAI_SOURCE_ARCHIVE_SHA256="903a231eca8f6a1f050953b603d5a278a1606abcdf47434eb1b45262d74068aa"
ARG JANKURAI_SOURCE_TREE="611229e54938c0e8808896e369fd54d095d258f7"
ARG JANKURAI_SEMVER="1.6.11"
ARG JANKURAI_TAG="v1.6.11-deadlang-precision-split.3"
ENV CARGO_HOME=/opt/rust/cargo
RUN apt-get update && apt-get install -y --no-install-recommends \
      git ca-certificates \
    && rm -rf /var/lib/apt/lists/*
# Same install invocation as ensure-jankurai.sh; the version assert fails the build if
# the rev and the version string ever drift apart.
RUN cargo install --git "$JANKURAI_REPO" --rev "$JANKURAI_REV" --locked --bin jankurai jankurai \
    && /opt/rust/cargo/bin/jankurai --version | grep -qx "$JANKURAI_VERSION"

# ---- runtime image -------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

ARG RUST_VERSION=1.95.0
ARG NODE_MAJOR=22
ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/opt/rust/cargo \
    RUSTUP_HOME=/opt/rust/rustup \
    PATH=/opt/jeryu/bin:/opt/rust/cargo/bin:/usr/local/bin:/usr/bin:/bin \
    JERYU_REAL_GIT=/usr/bin/git

# Base packages: ONLY what a Rust/Node build of this repo needs. No ssh, no docker CLI.
# `passwd` provides groupadd/useradd (the agent user is created below); it is not in the
# slim base. The rest is exactly what a Rust/Node build of this repo needs.
RUN apt-get update && apt-get install -y --no-install-recommends \
      git ca-certificates curl build-essential pkg-config libssl-dev \
      python3 passwd \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain (pinned), no docs/components beyond the build set.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}" \
    && /opt/rust/cargo/bin/rustup component add clippy rustfmt

# The pinned jankurai auditor (KEEP IN SYNC with ops/ci/ensure-jankurai.sh). The runtime
# is `--network none`, so the auditor must ship in the image. It lands at the exact path
# ops/ci/common.sh derives from CARGO_HOME (${CARGO_HOME}/bin/jankurai), so the CI lanes
# need zero changes, and JERYU_JANKURAI_BIN pins lanes to this binary — never a bare
# `jankurai` PATH lookup a stale build could shadow.
ARG JANKURAI_VERSION="jankurai 1.6.11"
COPY --from=jankurai-build /opt/rust/cargo/bin/jankurai /opt/rust/cargo/bin/jankurai
ENV JERYU_JANKURAI_BIN=/opt/rust/cargo/bin/jankurai
RUN /opt/rust/cargo/bin/jankurai --version | grep -qx "${JANKURAI_VERSION}"

# Node + the Vite/TS/React toolchain, then the agent CLIs.
RUN curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && npm install -g vite typescript \
       @openai/codex @anthropic-ai/claude-code \
    && rm -rf /var/lib/apt/lists/* /root/.npm

# Jekko (jeryu's own agent CLI) — installed from the internal registry/release.
# Kept as a clearly-marked install hook so the image build stays reproducible.
RUN npm install -g @jeryu/jekko-cli || echo "jekko: install from internal registry at build time"

# The git guard becomes the ONLY `git` on the agent PATH; a refusal wrapper is installed
# as every tool a "just edit + build + commit code in this branch" agent does NOT need.
# The agent reads/edits files and runs the build (cargo/rustc/rustup/node/npm/npx/tsc/
# vite) and commits via the git guard — nothing else. Wrapped tools fall into:
#   * networking (jeryu mediates all network + git):
#       gh curl wget ssh scp sftp nc ncat netcat telnet socat
#       git-remote-https git-remote-http
#   * symlinks (escape the workspace / link to host paths — never needed here): ln
#   * privilege escalation: sudo su doas
#   * package installs (the image is fixed; deps are prefetched):
#       apt apt-get aptitude dpkg pip pip3 gem
#   * container / orchestration: docker podman nerdctl kubectl ctr runc
#   * scheduling / daemons: crontab at batch systemctl service
#   * mount / disk / namespace escape: mount umount dd mkfs nsenter unshare chroot setarch
#   * keys / ownership / external-open: ssh-keygen ssh-add gpg chown xdg-open open
COPY --from=guard-build /jeryu-git /opt/jeryu/bin/git
COPY images/agent-sandbox/bin/refuse /opt/jeryu/bin/refuse
RUN for tool in \
      gh curl wget ssh scp sftp nc ncat netcat telnet socat \
      git-remote-https git-remote-http \
      ln \
      sudo su doas \
      apt apt-get aptitude dpkg pip pip3 gem \
      docker podman nerdctl kubectl ctr runc \
      crontab at batch systemctl service \
      mount umount dd mkfs nsenter unshare chroot setarch \
      ssh-keygen ssh-add gpg chown xdg-open open ; do \
      cp /opt/jeryu/bin/refuse "/opt/jeryu/bin/${tool}"; done \
    && chmod -R 0555 /opt/jeryu/bin

# The seccomp profile the runner references at /opt/jeryu/seccomp/<name>.json.
COPY images/agent-sandbox/seccomp/ /opt/jeryu/seccomp/

# Prefetch this repo's build dependencies so the agent's first build is warm and the
# container needs no network (it runs --network none). The cargo registry cache is taken
# from the guard-build stage (which had the FULL workspace source, so `cargo fetch` could
# resolve every member); the npm deps are installed here from the lockfile.
WORKDIR /opt/jeryu/repo
COPY --from=guard-build /opt/rust/cargo/registry /opt/rust/cargo/registry
COPY Cargo.toml Cargo.lock ./
COPY apps/web/package.json apps/web/package-lock.json ./apps/web/
RUN (cd apps/web && npm ci --no-audit --no-fund || true)

# Non-root user (uid/gid 1000). The container runs `--read-only`, so the ONLY writable
# spots are `--tmpfs /tmp` and the workspace bind-mount. The agent CLIs (codex/claude)
# write under $HOME (~/.codex, ~/.claude, ~/.cache); pointing HOME at a subdir of the
# writable workspace mount lets them start without touching the read-only root. The
# entrypoint creates $HOME on first use (a fresh workspace mount has no such dir yet),
# then execs the agent argv emitted by the runner (codex / claude / jekko ...).
COPY images/agent-sandbox/bin/entrypoint /opt/jeryu/bin/entrypoint
RUN chmod 0555 /opt/jeryu/bin/entrypoint
# groupadd/useradd live in /usr/sbin, which is intentionally NOT on the agent PATH above;
# call them by absolute path so the build does not depend on PATH.
RUN /usr/sbin/groupadd -g 1000 agent \
    && /usr/sbin/useradd -u 1000 -g 1000 -m -s /bin/bash agent \
    && mkdir -p /workspace && /bin/chown agent:agent /workspace
USER agent
WORKDIR /workspace
ENV HOME=/workspace/.agent-home TERM=xterm-256color JERYU_REAL_GIT=/usr/bin/git
ENTRYPOINT ["/opt/jeryu/bin/entrypoint"]
