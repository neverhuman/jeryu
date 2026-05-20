# Domain Repair Surface

Domain-facing failures must cross agent boundaries as typed repair hints, not
free-form prose. The required shape is:

```yaml
repair_hint:
  purpose: "what invariant or workflow this protects"
  reason: "the evidence-backed failure reason"
  common_fixes:
    - "smallest local fix to try first"
    - "next fix if the first does not apply"
  docs_url: "docs/testing.md#typed-repair-hint"
  repair_hint: "the narrow rerun command"
```

Keep domain code free of transport, persistence, and subprocess concerns. If a
domain rule needs RedlineDB evidence, route that through the owning adapter or
database boundary and return a typed repair hint to the caller.
