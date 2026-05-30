# Changelog

## Unreleased

- Fused workspace is under active local-CI hardening.
- Runner sandbox contract tests now cover seccomp, Landlock, cgroup, env scrub, and OCI socket denial.
- Local CI lanes default to 40 workers through `ops/ci/common.sh`.
