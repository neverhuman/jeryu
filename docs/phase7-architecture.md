# Phase 7 Architecture

Phase 7 joins three Rust-first subsystems:

1. `jeryu-proof` produces proof plans and proof witnesses from changed paths.
2. `jeryu-agentbridge` gives agents typed, scoped APIs for context, dry-run patches, proof plans, proof execution, proposed fixes, and hotfixes.
3. `jeryu-ci-scheduler` admits only proof-witnessed pull requests into a merge queue and validates speculative merge entries.

## Merge law

A pull request may enter the queue only when:

- It has a proof witness covering the head SHA.
- The witness covers every changed path.
- Required proof lanes passed.
- The queue can validate a speculative merge candidate without path conflicts.

## Agent law

Agents do not write directly. They must:

1. Request context.
2. Submit a dry-run patch within scope.
3. Receive a patch receipt.
4. Request a proof plan.
5. Run proof through typed APIs.
6. Attach the proof witness to a proposed fix or hotfix.
