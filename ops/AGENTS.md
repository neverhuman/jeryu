# Ops Agent Guidance

Owns: local CI wrappers (`ops/ci/`), pre-push hooks (`ops/git-hooks/`), and the
split-family tooling (`ops/split/`).

Forbidden: product source code (lives in the split member repositories) and
release artifact logic (release authority is `jeryu-deploy`).

Proof lanes: every gate is locally reproducible — `just fast`, `just check`,
`just score`, `just security`, `just artifact-support`, and the canonical PR
gate `bash ops/ci/pr-ci.sh` that host CI and the hosted workflow both run.
