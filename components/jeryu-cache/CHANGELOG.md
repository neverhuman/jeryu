# Changelog

## Unreleased

- Candidate split.2 uses `git.neverhuman.org` as source authority and fails
  closed on hosted proof, dependency, source, and release-evidence drift.
- Add a tested JSON Schema for the core Cache receipt and reject malformed
  digests during Serde deserialization instead of bypassing `Digest::parse`.

## jeryu-cache-v5.0.0-split.0 - 2026-06-11
- MAJOR: first standalone split-family release; the legacy monorepo
  (/home/ubuntu/jeryu) is deprecated and its drift fully reconciled.

## jeryu-cache-v4.0.0-split.0

- Initial split-family baseline for `jeryu-cache`.
