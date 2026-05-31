# Changelog

## Unreleased

- Fused workspace is under active local-CI hardening.
- Jankurai full audit is now score 98 with caps 0 and findings 0; tool adoption is artifact-verified for all 20 applicable catalog tools.
- Runner sandbox now has a live Docker-backed escape matrix covering no-new-privileges, default seccomp, cgroup memory/pid pressure, network denial, host socket absence, and workspace-only writable file isolation.
- Local CI lanes default to 40 workers through `ops/ci/common.sh`.
