# Docs Agent Guidance

Owns:
- Architecture, testing, error repair, boundary, generated-zone, audit, and
  release-control documentation.
- Keeping root `AGENTS.md` and `README.md` routed to the same canonical docs.

Forbidden:
- Hosted-provider or retired review-request terminology.
- Aspirational release claims without executable gate evidence.
- Generated artifact edits outside `agent/generated-zones.toml`.

Proof lane:
- `cargo run -q -p jeryu-mapcheck -- docs`
- `bash ci-fast-push.sh --no-push` before release-facing docs are signed.
