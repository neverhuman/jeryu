# JeRyu Web Forge — Markdown rendering & security

> Markdown is the most user-supplied surface in the Web Forge. README files,
> MR descriptions, comments, issue bodies, release notes, and evidence-pack
> notes all flow through the same renderer. This document captures the
> security posture: **server `ammonia` → client `DOMPurify` → trusted DOM**.
>
> **Source plan:** WEB_WORK_CLAUDE.md §35.1.4 (cache key versioning),
> §35.3.5 (allow-list), §35.1.7 (README lookup order), §35.1.8 (generic
> render endpoint), §35.1.9 (`/raw` vs `/blob`), §35.1.10 (path safety),
> §35.1.19 (binary/SVG/large-file rules).
>
> **Test corpus:** [`tests/web_markdown_tests.rs`](../tests/web_markdown_tests.rs)
> — 21 tests covering XSS defenses + GFM contract.

---

## 1. Why double-sanitize?

A defense-in-depth posture: server `ammonia` defines the canonical allow
list; the client re-sanitizes with `DOMPurify` before mounting into the
DOM. This protects against:

1. **Cache poisoning of the renderer.** Even if a regression in the
   server pipeline emits a tag we did not intend, the client strips it
   before it reaches `innerHTML`.
2. **Transport tampering.** Proxies, intermediate caches, or
   misconfigured CDNs cannot inject script tags because the client's
   `DOMPurify` config is the same allow-list applied a second time.
3. **Renderer version drift.** When the client `DOMPurify` allow-list is
   tighter than the server (because of a v1.1 sanitizer bump), users
   are protected before the server cache is invalidated.
4. **Defense against future bugs in `pulldown-cmark` / `comrak`.** Even
   if the parser emits malformed HTML, `ammonia` walks the parse tree
   and rejects anything outside the policy.

Both pipelines are versioned independently so the cache stays correct
when either bumps.

---

## 2. Renderer and sanitizer versions

Two public Rust constants (`src/repo_browser/markdown.rs`):

```rust
pub const RENDERER_VERSION:  &str = "jeryu-md-renderer.v1";
pub const SANITIZER_VERSION: &str = "jeryu-md-sanitizer.v1";
```

| Constant | Bumps when … |
|---|---|
| `RENDERER_VERSION` | Parser options change (e.g. new GFM extension enabled, footnote rendering altered). |
| `SANITIZER_VERSION` | The `ammonia` allow/block list changes (e.g. tightening allowed `<svg>` shapes, dropping `<details>`). |

Both versions:

- Are returned in every `POST /api/v1/markdown/render` response so the
  client can re-render its DOMPurify config when the server bumps.
- Are baked into the cache key (§4) so a bump invalidates only the
  affected cells without flushing the entire cache.

---

## 3. Allow list

Verbatim from WEB_WORK_CLAUDE.md §35.3.5. The server `ammonia::Builder`
permits:

```
a   p   pre   code   blockquote
ul  ol  li
table thead tbody tr th td
h1 h2 h3 h4 h5 h6
img
details  summary
kbd
del  strong  em
hr   br
input        (task lists only: type=checkbox, disabled, checked)
```

Allowed attributes:

| Tag | Attributes |
|---|---|
| `a` | `href`, `title`, `rel`, `target` (set by post-processor; user `target` is dropped) |
| `img` | `src`, `alt`, `title`, `width`, `height` |
| `code`, `pre` | `class` (only `language-*`) |
| `td`, `th` | `align` (left, center, right) |
| `input` | `type=checkbox`, `disabled`, `checked` (task lists) |
| `details`, `summary` | (no attributes; collapsible block content) |

URL schemes allowed: `http`, `https`, `mailto`, plus relative URLs (`/`,
`./`, `../`). All other schemes are stripped.

The client `DOMPurify` config is generated from the same Rust list to
keep server and browser in lockstep; the codegen runs alongside
`contracts/generated/*.ts`.

---

## 4. Block list

Stripped before output. Any document containing these in input is
rendered without the offending fragment; the fragment is replaced by an
HTML comment so reviewers can see where content was removed.

| Surface | Stripped | Why |
|---|---|---|
| Tags | `script`, `style`, `iframe`, `object`, `embed`, `form`, `link`, `meta`, `template`, `slot`, `svg`, `math` | Active content or untrusted vector graphics. |
| Event handlers | `on*` (`onclick`, `onerror`, `onload`, every variant) | Active content. |
| URL schemes | `javascript:`, `vbscript:`, `data:` (except `data:image/{png,jpeg,gif,webp}` *if* image proxy off — currently off in v1, so blocked), `file:`, `about:` | Active content / local resource access. |
| Attributes | `style`, `formaction`, `srcset`, `srcdoc`, `xlink:href` | Style injection, CSS escape, foreign-element injection. |
| Comments | (preserved) | HTML comments are kept so we can mark stripped sections. |

`<svg>` is **download-only** in v1. v1.5 may reintroduce sanitized SVG
with a strict allow-list (`<svg viewBox>` + `<path d>` only); until then,
inline SVG is treated as a `script` equivalent and removed.

---

## 5. Relative link rewriting

When a Markdown document references a relative path (`./guide.md`,
`../images/logo.png`, `#anchor`), the renderer rewrites it for the SPA:

| Input | Rewritten to | Notes |
|---|---|---|
| `./guide.md` | `/repos/{repo_id}/blob/{ref}/{normalized_path}` | Same ref as the current document. |
| `../docs/intro.md` | `/repos/{repo_id}/blob/{ref}/docs/intro.md` | Path resolved against the document's directory. |
| `images/logo.png` | `/api/v1/repos/{repo_id}/raw?ref={ref}&path=images/logo.png` | Served via the BFF raw endpoint with the viewer's auth. |
| `#section` | `#section` | Preserved as-is (consumed by the in-page TOC). |

Path normalisation rejects `..` escapes that would leave the repository
root, leading `/`, NUL bytes, and backslashes (§35.1.10). The
`MarkdownContext { repo, ref, current_path }` struct carries the
necessary state.

---

## 6. External link policy

Absolute `http://` / `https://` links are preserved but rewritten:

```html
<a href="https://example.com" rel="noopener noreferrer" target="_blank">…</a>
```

- `rel="noopener noreferrer"` is added unconditionally.
- `target="_blank"` is added unconditionally.
- Any user-supplied `target` or `rel` is overwritten (we do not honor
  `target="_top"`, `target="_parent"`, etc.).
- An optional `<span class="external-link-indicator">↗</span>` icon is
  appended at render time so the SPA's CSS can style external links
  consistently.

---

## 7. Image policy

| Source | Policy |
|---|---|
| Relative path (`./logo.png`) | Routed through the BFF raw endpoint (§5) with the viewer's auth. |
| Absolute `https://` URL | Currently passed through (`src` preserved). v1.5 may proxy through `/api/v1/image-proxy` to remove the referrer and cache. |
| `data:` URI | **Forbidden** in v1. Stripped. |
| `<svg>` inline | **Forbidden** in v1. Stripped. |
| Markdown size cap | Per-document input cap of 1 MiB (§9). Images themselves are not size-capped at the renderer; the SPA enforces a 5 MiB render-side cap and shows a placeholder above that. |

---

## 8. Cache key

The `web_markdown_cache` table (W-F-05 migration) keys each cell on:

```
(repo_id, commit_sha, path, blob_sha, renderer_version, sanitizer_version)
```

| Field | Source |
|---|---|
| `repo_id` | Stable UUID-shaped id (`web_repositories.id`). |
| `commit_sha` | Resolved commit, not the symbolic ref. |
| `path` | Path normalized via §5. |
| `blob_sha` | Git blob sha — the content fingerprint. |
| `renderer_version` | `RENDERER_VERSION` constant at render time. |
| `sanitizer_version` | `SANITIZER_VERSION` constant at render time. |

The primary key is `(repo_id, commit_sha, path, renderer_version,
sanitizer_version)`; `blob_sha` is a non-key column used to detect
file-replacement at the same commit (rare; usually a force-push scenario).

Cache hit latency target: **< 25 ms** (read from SQLite). Cache miss
target on a README-sized document: **< 150 ms** (parse + sanitize +
write). Both budgets come from WEB_WORK_CLAUDE.md §18.

Eviction: nightly job removes rows whose `renderer_version` or
`sanitizer_version` no longer matches the active constants.

---

## 9. DoS protections

| Limit | Value | Behaviour |
|---|---:|---|
| Input size | 1 MiB | `413` with `validation_failed` and `details.field: "markdown"`. |
| Heading depth | 7 (h1–h6 + virtual TOC root) | Deeper headings are flattened to h6. |
| Table dimensions | 200 cols × 5000 rows | Excess truncated; a footer node added. |
| Image count per doc | 200 | Excess replaced with a placeholder. |
| Render wall-clock | 750 ms tokio timeout | Returns `502 upstream_unavailable` (renderer treated as an "upstream" subsystem). Telemetry tag `markdown_render_timeout`. |
| Concurrency | tokio semaphore, 16 parallel renders per process | Excess queues; back-pressure surfaced via `markdown_render_queue_depth`. |

The 1 MiB cap is enforced at the BFF middleware (request body) **and**
inside the renderer (in case a large blob is fetched server-side from a
host adapter); both checkpoints emit the same `validation_failed`
envelope.

---

## 10. Binary and SVG rules

Per WEB_WORK_CLAUDE.md §35.1.19:

- Binary blobs are **rejected** by the renderer. The `BlobResponse` API
  carries `is_binary: bool`; if `render=md` is requested for a binary
  file the API returns `400 validation_failed`.
- SVG inline content is treated as `script` for v1; the file is
  download-only via `/api/v1/repos/{repo_id}/raw`.
- Large files do not lock the browser: the SPA caps rendered output at 2
  MiB of HTML; documents over the cap show a "View full document"
  fallback that streams the raw bytes.

---

## 11. Sample payloads

### 11.1 `POST /api/v1/markdown/render`

```bash
curl -sS \\
  --cookie '__Host-jeryu-session=...' \\
  --header 'X-CSRF-Token: ...' \\
  --header 'Content-Type: application/json' \\
  --data '{ "markdown": "# hi\n\n<script>alert(1)</script>",
            "context": { "repo_id": "repo-uuid", "ref": "main" } }' \\
  http://127.0.0.1:8787/api/v1/markdown/render
```

Response:

```json
{
  "html": "<h1>hi</h1>",
  "renderer_version": "jeryu-md-renderer.v1",
  "sanitizer_version": "jeryu-md-sanitizer.v1"
}
```

### 11.2 README fetch

```bash
curl -sS \\
  --cookie '__Host-jeryu-session=...' \\
  'http://127.0.0.1:8787/api/v1/repos/repo-uuid/readme?ref=main'
```

Response (truncated):

```json
{
  "repo": { "id": "repo-uuid", "full_name": "veox/jeryu" },
  "path": "README.md",
  "ref_name": "main",
  "sha": "abc...",
  "size_bytes": 21504,
  "mime": "text/markdown",
  "encoding": "utf8",
  "is_binary": false,
  "text": "# JeRyu …",
  "rendered_markdown": {
    "html": "...",
    "headings": [ { "depth": 1, "id": "jeryu", "text": "JeRyu" } ],
    "links": [],
    "renderer_version": "jeryu-md-renderer.v1",
    "sanitizer_version": "jeryu-md-sanitizer.v1"
  }
}
```

---

## 12. XSS / GFM test corpus

The 21 tests in [`tests/web_markdown_tests.rs`](../tests/web_markdown_tests.rs)
exercise both directions:

**GFM contract:**

- Tables render `<table><thead><tbody>` with `<th>`/`<td>` cells.
- Task lists render `<input type="checkbox" disabled>` with `checked`
  preserved on `- [x]`.
- Strikethrough (`~~x~~`) renders as `<del>`.
- Footnotes render with anchor + back-anchor.
- Autolinks render `<a href="https://…">`.
- Smart punctuation is enabled.
- Headings render with deterministic slug ids (`#user-content-<slug>`).
- Inline code preserves `&` / `<` / `>` HTML-entity encoded.

**XSS posture:**

- `<script>` is stripped wholesale.
- `<img onerror=…>` is rejected (event handler attribute removed).
- `javascript:` href is rejected.
- `data:text/html` href is rejected.
- Inline `<style>` is stripped.
- `<iframe>`, `<object>`, `<embed>` are stripped.
- `<form>`, `<input>` outside task lists are stripped.
- Inline `<svg>` is stripped (download-only policy).
- `<a target="_top">` is rewritten to `target="_blank" rel="noopener noreferrer"`.
- `data:image/svg+xml` images are stripped (no inline SVG via data URI).
- Markdown >1 MiB returns `413` with `validation_failed`.

Run the corpus locally:

```bash
cargo test --features web --test web_markdown_tests
```

Add a new fixture by appending a `#[test]` to the same file; the harness
shape is bytes-equal assertions on the rendered HTML, deliberately
avoiding any HTML parser so the test surface is exactly what the SPA
receives.
