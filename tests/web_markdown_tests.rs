//! W-T-01 — Markdown renderer XSS + GFM contract tests.
//!
//! Covers WEB_WORK_CLAUDE.md §7.5 W-T-01: every GFM feature emitted by
//! W-B-08 plus the XSS corpus that the §35.3.5 allow-list must defang.
//!
//! Each test uses simple substring assertions against the rendered HTML —
//! we deliberately avoid parsing the HTML so the test surface remains
//! exactly what bytes the SPA receives.

use jeryu::repo_browser::{render_markdown, MarkdownContext, MarkdownError};

fn ctx() -> MarkdownContext<'static> {
    MarkdownContext {
        repo_id: "repo-abc",
        ref_name: "main",
        current_path: "README.md",
    }
}

fn render(md: &str) -> String {
    render_markdown(md, &ctx()).expect("render").html
}

// ── GFM ─────────────────────────────────────────────────────────────────

#[test]
fn gfm_tables_render_as_table_thead_tbody() {
    let md = "\
| h1 | h2 |
|----|----|
| a  | b  |
";
    let html = render(md);
    assert!(html.contains("<table>"), "missing <table>: {html}");
    assert!(html.contains("<thead>"), "missing <thead>: {html}");
    assert!(html.contains("<tbody>"), "missing <tbody>: {html}");
    assert!(html.contains("<th>h1</th>"), "missing th cell: {html}");
    assert!(html.contains("<td>a</td>"), "missing td cell: {html}");
}

#[test]
fn task_list_renders_input_checkbox_with_checked_disabled() {
    let md = "\
- [x] done
- [ ] todo
";
    let html = render(md);
    assert!(
        html.contains("<input"),
        "task list lost input: {html}"
    );
    assert!(html.contains("type=\"checkbox\""), "no checkbox type: {html}");
    assert!(html.contains("disabled"), "no disabled attr: {html}");
    assert!(html.contains("checked"), "no checked attr: {html}");
}

#[test]
fn strikethrough_renders_as_del() {
    let html = render("~~gone~~");
    assert!(html.contains("<del>gone</del>"), "no <del>: {html}");
}

#[test]
fn footnotes_render_with_anchors() {
    let md = "\
A statement[^1].

[^1]: The footnote body.
";
    let html = render(md);
    // pulldown-cmark emits <sup class=\"footnote-reference\"><a href=\"#...\">…</a></sup>
    // and a corresponding `<div class=\"footnote-definition\" id=\"...\">` block.
    // After sanitize the `class=` attribute is stripped but the anchor with
    // `href="#..."` survives.
    assert!(
        html.contains("href=\"#1\"") || html.contains("href=\"#fn1\"") || html.contains("href=\"#"),
        "no footnote anchor: {html}"
    );
    assert!(html.contains("The footnote body"), "footnote body missing: {html}");
}

#[test]
fn headings_collect_toc_with_unique_ids() {
    let md = "\
# Setup
## Setup
### Setup
";
    let out = render_markdown(md, &ctx()).unwrap();
    assert_eq!(out.toc.len(), 3, "toc size: {:?}", out.toc);
    assert_eq!(out.toc[0].id, "setup");
    assert_eq!(out.toc[1].id, "setup-2");
    assert_eq!(out.toc[2].id, "setup-3");
    // ids must be injected into the rendered HTML
    assert!(out.html.contains("id=\"setup\""), "html missing setup id: {}", out.html);
    assert!(out.html.contains("id=\"setup-2\""), "html missing setup-2: {}", out.html);
    assert!(out.html.contains("id=\"setup-3\""), "html missing setup-3: {}", out.html);
}

#[test]
fn relative_link_rewrites_to_jeryu_route() {
    let md = "[setup](./docs/setup.md)";
    let out = render_markdown(md, &ctx()).unwrap();
    let target = "/api/v1/repos/repo-abc/blob/main/docs/setup.md";
    assert!(
        out.html.contains(target),
        "html missing rewritten href {target}: {}",
        out.html
    );
    // Link metadata captures both the raw and resolved.
    let link = out.links.iter().find(|l| l.href == "./docs/setup.md").expect("link");
    assert_eq!(link.resolved_route.as_deref(), Some(target));
    assert!(!link.external);
}

#[test]
fn relative_image_rewrites_to_jeryu_route() {
    let md = "![logo](./assets/logo.png)";
    let html = render(md);
    let target = "/api/v1/repos/repo-abc/blob/main/assets/logo.png";
    assert!(
        html.contains(target),
        "html missing rewritten image src {target}: {html}"
    );
    assert!(html.contains("<img"), "no <img> tag: {html}");
}

#[test]
fn external_link_gets_rel_noopener_noreferrer() {
    let md = "[rust](https://rust-lang.org)";
    let out = render_markdown(md, &ctx()).unwrap();
    assert!(
        out.html.contains("rel=\"noopener noreferrer\""),
        "external link missing rel: {}",
        out.html
    );
    assert!(
        out.html.contains("target=\"_blank\""),
        "external link missing target=_blank: {}",
        out.html
    );
    let link = out.links.iter().find(|l| l.href == "https://rust-lang.org").expect("link");
    assert!(link.external);
    assert!(link.resolved_route.is_none());
}

// ── XSS ─────────────────────────────────────────────────────────────────

#[test]
fn script_tag_is_stripped() {
    let md = "Before <script>alert('xss')</script> After";
    let html = render(md);
    assert!(!html.contains("<script"), "script tag survived: {html}");
    assert!(!html.contains("alert('xss')"), "script body survived: {html}");
}

#[test]
fn img_onerror_event_handler_is_stripped() {
    let md = "<img src=\"x.png\" onerror=\"alert(1)\">";
    let html = render(md);
    assert!(!html.contains("onerror"), "onerror survived: {html}");
    assert!(!html.contains("alert(1)"), "handler body survived: {html}");
}

#[test]
fn anchor_with_javascript_url_is_stripped() {
    let md = "[click](javascript:alert(1))";
    let html = render(md);
    assert!(
        !html.to_ascii_lowercase().contains("javascript:"),
        "javascript: url survived: {html}"
    );
}

#[test]
fn iframe_is_stripped() {
    let md = "Hi <iframe src=\"https://evil.example\"></iframe> there";
    let html = render(md);
    assert!(!html.contains("<iframe"), "iframe survived: {html}");
    assert!(!html.contains("evil.example"), "iframe src survived: {html}");
}

#[test]
fn object_is_stripped() {
    let md = "Hi <object data=\"x.swf\"></object> there";
    let html = render(md);
    assert!(!html.contains("<object"), "object survived: {html}");
}

#[test]
fn embed_is_stripped() {
    let md = "Hi <embed src=\"x.swf\"> there";
    let html = render(md);
    assert!(!html.contains("<embed"), "embed survived: {html}");
}

#[test]
fn form_is_stripped() {
    let md = "Hi <form action=\"steal\"><input name=\"x\"></form> there";
    let html = render(md);
    assert!(!html.contains("<form"), "form survived: {html}");
    assert!(!html.contains("action=\"steal\""), "form action survived: {html}");
}

#[test]
fn style_attribute_is_stripped() {
    let md = "<p style=\"background:url(evil)\">x</p>";
    let html = render(md);
    assert!(
        !html.contains("style=") && !html.contains("background:url(evil)"),
        "style attr survived: {html}"
    );
}

#[test]
fn svg_with_script_is_stripped() {
    let md = "<svg><script>alert(1)</script></svg>";
    let html = render(md);
    assert!(!html.contains("<svg"), "svg survived: {html}");
    assert!(!html.contains("<script"), "svg-script survived: {html}");
    assert!(!html.contains("alert(1)"), "svg script body survived: {html}");
}

#[test]
fn data_url_image_is_stripped_or_allowed_only_for_safe_mime() {
    // We forbid all data: image URLs in v1 — see markdown.rs sanitize().
    // Use a clean data: URL (no embedded HTML that would break the parser).
    let md = "![pixel](data:image/png;base64,iVBORw0KGgo=)";
    let out = render_markdown(md, &ctx()).unwrap();
    // The <img> tag must lose its src (so it can't load the data: payload).
    assert!(
        !out.html.to_ascii_lowercase().contains("src=\"data:"),
        "data: src survived: {}",
        out.html
    );
    assert!(
        !out.html.to_ascii_lowercase().contains("src='data:"),
        "data: src (single quote) survived: {}",
        out.html
    );
    // Now confirm the dangerous variant with embedded script is fully neutered.
    let bad = "<img src=\"data:image/svg+xml;base64,PHN2ZyBvbmxvYWQ9YWxlcnQoMSk+\">";
    let html_bad = render(bad);
    assert!(
        !html_bad.to_ascii_lowercase().contains("data:"),
        "raw <img data:> survived: {html_bad}"
    );
}

// ── Metadata ────────────────────────────────────────────────────────────

#[test]
fn renderer_version_is_jeryu_md_renderer_v1() {
    let out = render_markdown("# x", &ctx()).unwrap();
    assert_eq!(out.renderer_version, "jeryu-md-renderer.v1");
}

#[test]
fn sanitizer_version_is_jeryu_md_sanitizer_v1() {
    let out = render_markdown("# x", &ctx()).unwrap();
    assert_eq!(
        out.sanitizer_version.as_deref(),
        Some("jeryu-md-sanitizer.v1")
    );
}

#[test]
fn markdown_over_1_mib_returns_error() {
    let big = "a".repeat(1_048_577);
    let err = render_markdown(&big, &ctx()).unwrap_err();
    match err {
        MarkdownError::TooLarge(n) => assert_eq!(n, 1_048_577),
        other => panic!("expected TooLarge, got {other:?}"),
    }
}
