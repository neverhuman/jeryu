# Ops Agent Guidance

Owns: local CI scripts, phase gates, security/audit/release lanes, runbooks, dashboards, and operational smoke tests.

Forbidden: do not add hosted-CI-only checks that cannot be run locally; do not mark live runtime gates as passing without executable evidence.

Proof lane: run `bash scripts/ci-phases.sh` for gate changes, `just security` for security-lane changes, and `just audit` for Jankurai/audit-lane changes.
