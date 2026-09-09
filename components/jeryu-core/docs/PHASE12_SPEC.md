# Phase 12 Spec — JeryuCache Cache/CAS

This phase implements the JeryuCache cache architecture described in the Jeryu spec.

## Deliverables

- repo-scoped compiled artifact CAS;
- source/registry blob CAS;
- tenant CAS;
- explicit shared cache scopes with opt-in policy;
- release-hermetic L6 vendor snapshots;
- receipts for restore/write/quarantine/promote;
- cache quarantine and promotion rules;
- false-hit detector;
- adversarial poisoning harness.

## Exit bar

Zero false hits across adversarial cache suite.
