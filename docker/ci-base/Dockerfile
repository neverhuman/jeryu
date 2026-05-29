# syntax=docker/dockerfile:1.7
# jeryu CI base image — one general runner image for all jeryu projects.
#
# Pinned toolchain (kept in sync with ops/ci/lib.sh + scripts/jankurai-manifest.json):
#   rust 1.95.0 · cargo-nextest 0.9.133 · cargo-deny 0.19.8
#   jankurai 1.5.1 (SHA256-verified)
#   actionlint v1.7.8 · gitleaks 8.30.0 · syft 1.40.0
#   node 20 · python3 · sqlite3
#
# NO sccache: every jeryu project independently disables it because the daemon
# zombies under cargo parallel spawn (jeryu, redlineDB, redline-testing, jansu
# all carry a `RUSTC_WRAPPER= SCCACHE_DISABLED=1` workaround). Baking the empty
# wrapper at image level fixes it at the source.
#
# Cache convention (volumes mounted by the runner):
#   /cache/cargo            CARGO_HOME with registry + git subdirs
#   /cache/git-clones       shared clone cache (flock at /cache/git-clones.lock)
#   /cache/apt-archives     persistent apt download cache
#   /cache/jankurai         jankurai run scratch
#
# Build:  bash docker/ci-base/build.sh
# Tag:    ${CI_REGISTRY}/root/jeryu/ci-base:1.95.0 and :latest

# ubuntu:24.04 (noble) chosen over debian:bookworm-slim because the prebuilt
# jankurai 1.5.1 binary is linked against GLIBC 2.39, which bookworm (GLIBC 2.36)
# does not provide. noble ships GLIBC 2.39 and otherwise has equivalent ergonomics.
FROM ubuntu:24.04

ARG RUST_VERSION=1.95.0
ARG NODE_MAJOR=20
ARG JANKURAI_VERSION=1.5.1
ARG JANKURAI_SHA256=a12dbb4a3805dee807fc101d4b073ac9386936b33c5579f606a655fe90d0bbac
ARG CARGO_NEXTEST_VERSION=0.9.133
ARG CARGO_DENY_VERSION=0.19.8
ARG ACTIONLINT_VERSION=1.7.8
ARG GITLEAKS_VERSION=8.30.0
ARG SYFT_VERSION=1.40.0

# CARGO_HOME stays at /usr/local/cargo during image build so rustup + cargo install
# land in a writable system path on PATH. The final stage redefines CARGO_HOME to
# /cache/cargo so runtime registry/git caches go to the persistent mount; the rustup
# proxies in /usr/local/cargo/bin remain on PATH and continue to dispatch correctly.
ENV DEBIAN_FRONTEND=noninteractive \
    LC_ALL=C.UTF-8 \
    LANG=C.UTF-8 \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    PATH=/usr/local/cargo/bin:/usr/local/rustup/toolchains/${RUST_VERSION}-x86_64-unknown-linux-gnu/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    RUSTC_WRAPPER= \
    SCCACHE_DISABLED=1 \
    JERYU_USE_CI_BASE=1 \
    JERYU_CI_BASE_VERSION=${RUST_VERSION}

# ── System packages ────────────────────────────────────────────────────────
RUN set -eux; \
    apt-get update -qq; \
    apt-get install -y -qq --no-install-recommends \
      build-essential \
      ca-certificates \
      clang \
      curl \
      file \
      git \
      gnupg \
      jq \
      libssl-dev \
      lld \
      mold \
      openssl \
      pkg-config \
      python3 \
      python3-pip \
      sqlite3 \
      tar \
      unzip \
      xz-utils \
      zlib1g-dev; \
    rm -rf /var/lib/apt/lists/*

# ── Node.js (NodeSource — Debian's nodejs is too old for the tooling) ──────
RUN set -eux; \
    curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" | bash -; \
    apt-get install -y -qq --no-install-recommends nodejs; \
    npm install --global --silent npm@latest; \
    node --version | grep -E "^v${NODE_MAJOR}\."; \
    npm --version; \
    rm -rf /var/lib/apt/lists/*

# ── Rust toolchain via rustup (minimal profile + fmt + clippy) ─────────────
RUN set -eux; \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --default-toolchain "${RUST_VERSION}" --profile minimal \
                  --component clippy --component rustfmt; \
    /usr/local/cargo/bin/rustup --version; \
    /usr/local/cargo/bin/rustc --version | grep -F "${RUST_VERSION}"; \
    /usr/local/cargo/bin/cargo --version; \
    chmod -R a+rwX "$RUSTUP_HOME" /usr/local/cargo

# ── cargo-nextest (pinned prebuilt tarball; no compile cost) ───────────────
RUN set -eux; \
    curl -fsSL "https://get.nexte.st/${CARGO_NEXTEST_VERSION}/linux" \
      | tar -C /usr/local/cargo/bin -xz; \
    cargo-nextest nextest --version | grep -F "${CARGO_NEXTEST_VERSION}"

# ── cargo-deny (compiled via cargo install; one-shot, cached in layer) ─────
RUN set -eux; \
    cargo install --locked cargo-deny --version "${CARGO_DENY_VERSION}" --root /usr/local; \
    cargo-deny --version | grep -F "${CARGO_DENY_VERSION}"

# ── Security tools (pinned, match ops/ci/lib.sh) ───────────────────────────
RUN set -eux; \
    curl -fsSL "https://github.com/rhysd/actionlint/releases/download/v${ACTIONLINT_VERSION}/actionlint_${ACTIONLINT_VERSION}_linux_amd64.tar.gz" \
      | tar -xz -C /usr/local/bin actionlint; \
    actionlint --version | grep -F "${ACTIONLINT_VERSION}"; \
    curl -fsSL "https://github.com/gitleaks/gitleaks/releases/download/v${GITLEAKS_VERSION}/gitleaks_${GITLEAKS_VERSION}_linux_x64.tar.gz" \
      | tar -xz -C /usr/local/bin gitleaks; \
    gitleaks version | grep -F "${GITLEAKS_VERSION}"; \
    curl -fsSL "https://github.com/anchore/syft/releases/download/v${SYFT_VERSION}/syft_${SYFT_VERSION}_linux_amd64.tar.gz" \
      | tar -xz -C /usr/local/bin syft; \
    syft --version | grep -F "${SYFT_VERSION}"

# ── jankurai (SHA256-verified per scripts/jankurai-manifest.json) ──────────
RUN set -eux; \
    cd /tmp; \
    curl -fsSL -o jankurai.tar.gz \
      "https://github.com/neverhuman/jankurai/releases/download/v${JANKURAI_VERSION}/jankurai-${JANKURAI_VERSION}-x86_64-unknown-linux-gnu.tar.gz"; \
    echo "${JANKURAI_SHA256}  jankurai.tar.gz" | sha256sum -c -; \
    tar -xzf jankurai.tar.gz; \
    install -m 0755 "jankurai-${JANKURAI_VERSION}-x86_64-unknown-linux-gnu/jankurai" /usr/local/bin/jankurai; \
    rm -rf /tmp/jankurai*; \
    jankurai --version | grep -F "${JANKURAI_VERSION}"

# ── Persistent cache mount points ──────────────────────────────────────────
RUN set -eux; \
    mkdir -p \
      /cache/cargo /cache/cargo/registry /cache/cargo/git \
      /cache/git-clones /cache/apt-archives /cache/jankurai; \
    chmod -R 777 /cache

# ── Switch CARGO_HOME to the persistent cache path for runtime ─────────────
# rustup proxies in /usr/local/cargo/bin remain on PATH; cargo's registry/git
# caches now go to /cache/cargo on the runner-mounted persistent volume.
ENV CARGO_HOME=/cache/cargo

# ── Final integration smoke (catches version drift at build time) ──────────
RUN set -eux; \
    rustc --version; \
    cargo --version; \
    cargo-nextest nextest --version; \
    cargo-deny --version; \
    actionlint --version; \
    gitleaks version; \
    syft --version; \
    jankurai --version; \
    node --version; \
    npm --version; \
    python3 --version; \
    sqlite3 -version; \
    git --version; \
    jq --version; \
    flock --version; \
    test -z "${RUSTC_WRAPPER}" && echo "RUSTC_WRAPPER unset as expected"

LABEL org.opencontainers.image.title="jeryu/ci-base" \
      org.opencontainers.image.description="Pinned one-image CI runtime for the jeryu fleet" \
      jeryu.rust.version="${RUST_VERSION}" \
      jeryu.jankurai.version="${JANKURAI_VERSION}" \
      jeryu.node.major="${NODE_MAJOR}" \
      jeryu.cargo_nextest.version="${CARGO_NEXTEST_VERSION}" \
      jeryu.cargo_deny.version="${CARGO_DENY_VERSION}" \
      jeryu.actionlint.version="${ACTIONLINT_VERSION}" \
      jeryu.gitleaks.version="${GITLEAKS_VERSION}" \
      jeryu.syft.version="${SYFT_VERSION}"

WORKDIR /workspace
CMD ["bash"]
