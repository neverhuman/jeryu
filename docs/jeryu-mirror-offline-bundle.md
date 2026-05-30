# Jeryu Mirror offline bundle format v1

A Jeryu Mirror bundle is a directory with a deterministic manifest and archive:

```text
JERYU_BUNDLE
manifest.json
archive.json
docs/restore-instructions.md
repos/<owner>/<repo>/repository.json
repos/<owner>/<repo>/issues.json
repos/<owner>/<repo>/pull_requests.json
repos/<owner>/<repo>/releases.json
repos/<owner>/<repo>/artifacts.json
repos/<owner>/<repo>/webhooks.json
repos/<owner>/<repo>/apps.json
```

`manifest.json` contains:

- bundle format
- bundle id
- source descriptor
- object counts
- archive digest
- file list
- restore instruction path

`archive.json` is the canonical product truth. Per-repository files are included
for human inspection, partial restore tooling, and offline review.

## Integrity

`jeryu-mirror verify --bundle <path>` recalculates the `archive.json` digest and
checks the manifest file list. Restore should refuse bundles that fail this
verification.

## Secret Handling

Secret values are not present in the bundle. Webhooks and app installations store
only target secret names such as `jeryu-mirror/webhook/123`.
