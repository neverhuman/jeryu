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
#   * the jeryu-git guard installed AS `git` (deny-by-default allowlist) + refusal wrappers
#     for gh/curl/wget/ssh/scp on a controlled PATH,
#   * a non-root user `agent` (uid/gid 1000).
# Credentials are NEVER baked in — they are injected per-run (see jeryu-agent-auth).
#
# Build:  docker build -f images/agent-sandbox/Dockerfile -t localhost/jeryu/agent-sandbox:latest .
#         (the runner references it via JERYU_AGENT_IMAGE)

# ---- toolchain build stage: compile the jeryu-git guard from the repo ----------------
FROM rust:1.95-slim-bookworm AS guard-build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates/jeryu-git-guard ./crates/jeryu-git-guard
# Build only the guard binary; it is pure std, so this is fast and self-contained.
RUN cargo build --release --manifest-path crates/jeryu-git-guard/Cargo.toml --bin jeryu-git \
    && cp target/release/jeryu-git /jeryu-git

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
RUN apt-get update && apt-get install -y --no-install-recommends \
      git ca-certificates curl build-essential pkg-config libssl-dev \
      python3 \
    && rm -rf /var/lib/apt/lists/*

# Rust toolchain (pinned), no docs/components beyond the build set.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}" \
    && /opt/rust/cargo/bin/rustup component add clippy rustfmt

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
# container needs no network (it runs --network none).
WORKDIR /opt/jeryu/repo
COPY Cargo.toml Cargo.lock ./
COPY apps/web/package.json apps/web/package-lock.json ./apps/web/
RUN cargo fetch --manifest-path Cargo.toml || true \
    && (cd apps/web && npm ci --no-audit --no-fund || true)

# Non-root user; the agent's HOME is a writable tmpfs subtree at runtime.
RUN groupadd -g 1000 agent && useradd -u 1000 -g 1000 -m -s /bin/bash agent \
    && mkdir -p /workspace && chown agent:agent /workspace
USER agent
WORKDIR /workspace
ENV HOME=/home/agent TERM=xterm-256color JERYU_REAL_GIT=/usr/bin/git
