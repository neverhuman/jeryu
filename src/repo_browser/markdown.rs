//! Owner: Web Forge BFF — Markdown rendering pipeline (W-B-08).
//! Proof: `cargo nextest run -p jeryu --test web_markdown_tests`
//! Invariants:
//! - `RENDERER_VERSION` and `SANITIZER_VERSION` are stable identifiers; bump
//!   exactly one when behavior changes so cached HTML invalidates correctly
//!   (§35.1.4).
//! - All rendered HTML is sanitized through ammonia with the §35.3.5
//!   allow-list before it leaves this module.
//! - Hard cap at 1 MiB of input markdown (§33 risk register — markdown DoS).
//!
//! Source: WEB_WORK_CLAUDE.md §7.2 W-B-08 + §28.1 stub + §35.1.4 +
//! §35.3.5.

use std::borrow::Cow;
use std::collections::HashMap;

use ammonia::{Builder, UrlRelative};
use chrono::Utc;
use pulldown_cmark::{html, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::api::repo_browser::{MarkdownHeading, MarkdownLink, RenderedMarkdown};

/// Identity of the parser/options pipeline. Bump when pulldown-cmark options
/// or post-processing behavior change (§35.1.4).
pub const RENDERER_VERSION: &str = "jeryu-md-renderer.v1";

/// Identity of the ammonia allow-list. Bump independently from
/// `RENDERER_VERSION` when the sanitizer policy changes (§35.1.4).
pub const SANITIZER_VERSION: &str = "jeryu-md-sanitizer.v1";

/// Hard input cap to prevent renderer DoS (§33). 1 MiB.
const MAX_MARKDOWN_BYTES: usize = 1_048_576;

/// Context required to rewrite relative links/images to authenticated
/// JeRyu blob routes.
#[derive(Debug, Clone, Copy)]
pub struct MarkdownContext<'a> {
    /// Opaque repository id (matches §35.1.2 `RepositoryId.id`).
    pub repo_id: &'a str,
    /// Branch name, tag name, or SHA used in the route segment.
    pub ref_name: &'a str,
    /// Path of the markdown file inside the repo (e.g. `docs/setup.md`).
    /// Used to resolve relative links against the file's directory.
    pub current_path: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum MarkdownError {
    #[error("markdown too large: {0} bytes")]
    TooLarge(usize),
    #[error("render failed: {0}")]
    RenderFailed(String),
}

/// Render a markdown document to sanitized HTML with link metadata.
///
/// Returns a `RenderedMarkdown` DTO containing the cleaned HTML, the table of
/// contents (in document order), the full link list (with `external` flag and
/// `resolved_route` for relative targets), and both renderer/sanitizer
/// version identifiers.
pub fn render_markdown(
    md: &str,
    ctx: &MarkdownContext<'_>,
) -> Result<RenderedMarkdown, MarkdownError> {
    if md.len() > MAX_MARKDOWN_BYTES {
        return Err(MarkdownError::TooLarge(md.len()));
    }

    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_SMART_PUNCTUATION);

    // First pass: collect headings (for TOC + anchor injection) and link
    // metadata. We rewrite link destinations inline so the emitted HTML
    // already targets the JeRyu blob route for relative paths.
    let mut headings: Vec<MarkdownHeading> = Vec::new();
    let mut id_counts: HashMap<String, u32> = HashMap::new();
    let mut links: Vec<MarkdownLink> = Vec::new();

    // Stack-tracked heading buffer (the current open heading, if any).
    let mut current_heading: Option<(u8, String)> = None;

    let parser = Parser::new_ext(md, opts);
    let mut rewritten_events: Vec<Event<'_>> = Vec::with_capacity(256);

    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, id, classes, attrs }) => {
                current_heading = Some((heading_depth(level), String::new()));
                rewritten_events.push(Event::Start(Tag::Heading { level, id, classes, attrs }));
            }
            Event::End(TagEnd::Heading(level)) => {
                if let Some((depth, text)) = current_heading.take() {
                    let slug = unique_slug(&text, &mut id_counts);
                    headings.push(MarkdownHeading { depth, id: slug, text });
                }
                rewritten_events.push(Event::End(TagEnd::Heading(level)));
            }
            Event::Text(ref t) => {
                if let Some((_, ref mut buf)) = current_heading {
                    buf.push_str(t);
                }
                rewritten_events.push(event.clone());
            }
            Event::Code(ref t) => {
                if let Some((_, ref mut buf)) = current_heading {
                    buf.push_str(t);
                }
                rewritten_events.push(event.clone());
            }
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
                let raw = dest_url.to_string();
                let external = is_external(&raw);
                // Dangerous schemes never get treated as relative — pass them
                // through verbatim so ammonia's url_schemes filter strips them.
                let dangerous = is_dangerous_url(&raw);
                let resolved = if dangerous || external || raw.starts_with('#') || raw.is_empty() {
                    None
                } else {
                    resolve_relative_route(&raw, ctx)
                };
                let new_dest = resolved.clone().unwrap_or_else(|| raw.clone());
                links.push(MarkdownLink {
                    href: raw,
                    resolved_route: resolved,
                    external,
                });
                rewritten_events.push(Event::Start(Tag::Link {
                    link_type,
                    dest_url: CowStr::Boxed(new_dest.into_boxed_str()),
                    title,
                    id,
                }));
            }
            Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
                let raw = dest_url.to_string();
                let external = is_external(&raw);
                let dangerous = is_dangerous_url(&raw);
                let resolved = if dangerous || external || raw.starts_with('#') || raw.is_empty() {
                    None
                } else {
                    resolve_relative_route(&raw, ctx)
                };
                let new_dest = resolved.unwrap_or_else(|| raw.clone());
                rewritten_events.push(Event::Start(Tag::Image {
                    link_type,
                    dest_url: CowStr::Boxed(new_dest.into_boxed_str()),
                    title,
                    id,
                }));
            }
            other => rewritten_events.push(other),
        }
    }

    let mut raw_html = String::new();
    html::push_html(&mut raw_html, rewritten_events.into_iter());

    // Inject heading anchors (id="slug") BEFORE sanitization so the post-
    // sanitize HTML carries them through. We attach attributes only to the
    // raw <hN> tags we ourselves emitted; user-supplied HTML <hN> tags will
    // get dropped/cleaned by ammonia anyway.
    let with_anchors = inject_heading_anchors(&raw_html, &headings);

    let clean = sanitize(&with_anchors);

    Ok(RenderedMarkdown {
        html: clean,
        toc: headings,
        links,
        renderer_version: RENDERER_VERSION.to_string(),
        sanitizer_version: Some(SANITIZER_VERSION.to_string()),
        rendered_at: Utc::now(),
    })
}

fn heading_depth(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// `lowercase`, non-alphanumeric → `-`, collapse runs, trim leading/trailing
/// dashes. Identical to GitHub's slugger for ASCII inputs.
fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_dash = true; // skip leading dashes
    for c in s.chars() {
        if c.is_alphanumeric() {
            for ch in c.to_lowercase() {
                out.push(ch);
            }
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        out.push_str("section");
    }
    out
}

/// De-duplicate slugs across the document by appending `-2`, `-3`, …
fn unique_slug(text: &str, seen: &mut HashMap<String, u32>) -> String {
    let base = slugify(text);
    let count = seen.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{}", *count)
    }
}

fn is_external(href: &str) -> bool {
    href.starts_with("http://")
        || href.starts_with("https://")
        || href.starts_with("//")
        || href.starts_with("mailto:")
}

/// Resolve a relative href against `ctx.current_path` and produce the JeRyu
/// blob route `/api/v1/repos/{repo_id}/blob/{ref}/{normalized_path}`.
/// Returns `None` for inputs that can't be resolved (currently never, but the
/// signature keeps room for future strictness — e.g. rejecting `../` escapes).
fn resolve_relative_route(href: &str, ctx: &MarkdownContext<'_>) -> Option<String> {
    let normalized = normalize_relative(ctx.current_path, href)?;
    Some(format!(
        "/api/v1/repos/{}/blob/{}/{}",
        ctx.repo_id, ctx.ref_name, normalized
    ))
}

/// Normalize `target` relative to `base` (the markdown file's own path).
/// Handles `./`, `../`, and bare segments. Does not collapse beyond the repo
/// root — `..` past root produces an empty path component, which is harmless
/// because the consumer is the BFF blob route.
fn normalize_relative(base: &str, target: &str) -> Option<String> {
    let target = target.trim_start_matches("./");
    // Base directory: everything before the last `/` in `base`.
    let base_dir: Vec<&str> = match base.rfind('/') {
        Some(idx) => base[..idx].split('/').filter(|s| !s.is_empty()).collect(),
        None => Vec::new(),
    };
    let mut parts: Vec<String> = base_dir.into_iter().map(String::from).collect();
    for seg in target.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            other => parts.push(other.to_string()),
        }
    }
    Some(parts.join("/"))
}

/// Add `id="<slug>"` to each `<hN>` opening tag in document order. We pair
/// each emitted `<hN>` with the next collected heading; user-injected raw
/// HTML headings (which won't be in the TOC) get no id but will still
/// survive sanitize.
fn inject_heading_anchors(html: &str, headings: &[MarkdownHeading]) -> String {
    let mut out = String::with_capacity(html.len() + headings.len() * 16);
    let mut head_iter = headings.iter();
    let bytes = html.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Look for `<hN>` where N is 1..=6 and next byte is `>`.
        if bytes[i] == b'<'
            && i + 3 < bytes.len()
            && (bytes[i + 1] == b'h' || bytes[i + 1] == b'H')
            && (b'1'..=b'6').contains(&bytes[i + 2])
            && bytes[i + 3] == b'>'
        {
            let level = bytes[i + 2] - b'0';
            if let Some(h) = head_iter.next() {
                if h.depth == level {
                    out.push('<');
                    out.push(bytes[i + 1] as char);
                    out.push(bytes[i + 2] as char);
                    out.push_str(" id=\"");
                    push_attr_escaped(&mut out, &h.id);
                    out.push_str("\">");
                    i += 4;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn push_attr_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '"' => out.push_str("&quot;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
}

/// Sanitize HTML with the broader §35.3.5 allow-list. The default ammonia
/// policy already covers most of our needed tags; we only need to add `input`
/// (task lists) and the heading-anchor `id` attribute. We restrict the URL
/// schemes to `http`/`https`/`mailto` to reject `javascript:` and `data:`.
/// `script` is in the default `clean_content_tags`, so it is stripped along
/// with its inner text. `iframe`, `object`, `embed`, `form`, `style`, and
/// `svg` are not in the default tag allow-list, so they are stripped.
fn sanitize(input: &str) -> String {
    use std::collections::HashSet;

    let mut url_schemes: HashSet<&'static str> = HashSet::new();
    url_schemes.insert("http");
    url_schemes.insert("https");
    url_schemes.insert("mailto");

    let mut clean_content: HashSet<&'static str> = HashSet::new();
    // `script` and `style` are the defaults — preserve them and add the rest
    // explicitly so the sanitizer obliterates their contents too.
    clean_content.insert("script");
    clean_content.insert("style");
    clean_content.insert("svg");
    clean_content.insert("iframe");
    clean_content.insert("object");
    clean_content.insert("embed");
    clean_content.insert("form");

    Builder::default()
        .add_tags(["input"])
        .add_generic_attributes(["id"])
        // `rel` is managed by ammonia's `link_rel` setting (defaults to
        // "noopener noreferrer"); panics if we list it here.
        .add_tag_attributes("a", ["href", "title", "target"])
        .add_tag_attributes("img", ["src", "alt", "title", "width", "height"])
        .add_tag_attributes("input", ["type", "checked", "disabled"])
        .url_schemes(url_schemes)
        .clean_content_tags(clean_content)
        .url_relative(UrlRelative::PassThrough)
        // External links: ammonia adds `rel="noopener noreferrer"` by default.
        // We additionally inject `target="_blank"` via an attribute filter.
        .attribute_filter(|element, attribute, value| {
            // Stamp external <a> with target="_blank".
            if element == "a" && attribute == "target" {
                // Only allow `_blank` as a target value; other values get dropped.
                if value == "_blank" {
                    Some(Cow::Borrowed("_blank"))
                } else {
                    None
                }
            } else if element == "a" && attribute == "href" {
                // Belt-and-braces: drop dangerous protocols even though
                // url_schemes should already have done it.
                let trimmed = value.trim();
                if is_dangerous_url(trimmed) {
                    None
                } else {
                    Some(Cow::Owned(value.to_string()))
                }
            } else if element == "img" && attribute == "src" {
                let trimmed = value.trim();
                if is_dangerous_url(trimmed) || trimmed.starts_with("data:") {
                    None
                } else {
                    Some(Cow::Owned(value.to_string()))
                }
            } else {
                Some(Cow::Owned(value.to_string()))
            }
        })
        .clean(&inject_external_targets(input))
        .to_string()
}

/// Stamp `target="_blank"` onto every `<a href="http(s)://...">` so the
/// attribute_filter sees it. We do this pre-sanitize because ammonia's
/// `set_tag_attribute_values` API is more invasive than a simple scan.
fn inject_external_targets(html: &str) -> String {
    let mut out = String::with_capacity(html.len() + 32);
    let lower = html.to_ascii_lowercase();
    let bytes = html.as_bytes();
    let lbytes = lower.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Find `<a `.
        if i + 3 <= bytes.len() && &lbytes[i..i + 3] == b"<a " {
            // Find tag end.
            if let Some(end_off) = lower[i..].find('>') {
                let tag_end = i + end_off;
                let tag = &html[i..=tag_end];
                let tag_lower = &lower[i..=tag_end];
                // Check if href looks external.
                let has_ext = tag_lower.contains("href=\"http://")
                    || tag_lower.contains("href=\"https://")
                    || tag_lower.contains("href='http://")
                    || tag_lower.contains("href='https://");
                let has_target = tag_lower.contains(" target=");
                if has_ext && !has_target {
                    // Insert ` target="_blank"` before the closing `>`.
                    out.push_str(&tag[..tag.len() - 1]);
                    out.push_str(" target=\"_blank\">");
                } else {
                    out.push_str(tag);
                }
                i = tag_end + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn is_dangerous_url(s: &str) -> bool {
    // Case-insensitive prefix check for known unsafe schemes. Whitespace and
    // control characters inside the scheme have tricked sanitizers before
    // (`java\tscript:`), so we strip those before comparing.
    let cleaned: String = s
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_control())
        .collect();
    let lower = cleaned.to_ascii_lowercase();
    lower.starts_with("javascript:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("data:")
        || lower.starts_with("file:")
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> MarkdownContext<'static> {
        MarkdownContext {
            repo_id: "repo-abc",
            ref_name: "main",
            current_path: "README.md",
        }
    }

    #[test]
    fn renderer_emits_version_constants() {
        let out = render_markdown("# hi", &ctx()).unwrap();
        assert_eq!(out.renderer_version, "jeryu-md-renderer.v1");
        assert_eq!(out.sanitizer_version.as_deref(), Some("jeryu-md-sanitizer.v1"));
    }

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("  multi  space"), "multi-space");
        assert_eq!(slugify("!!"), "section");
    }

    #[test]
    fn unique_slug_dedupes() {
        let mut seen = HashMap::new();
        assert_eq!(unique_slug("Setup", &mut seen), "setup");
        assert_eq!(unique_slug("Setup", &mut seen), "setup-2");
        assert_eq!(unique_slug("Setup", &mut seen), "setup-3");
    }

    #[test]
    fn rejects_oversize_input() {
        let big = "a".repeat(MAX_MARKDOWN_BYTES + 1);
        let err = render_markdown(&big, &ctx()).unwrap_err();
        assert!(matches!(err, MarkdownError::TooLarge(_)));
    }

    #[test]
    fn normalize_relative_resolves_dotdot() {
        let r = normalize_relative("docs/setup/foo.md", "../images/x.png").unwrap();
        assert_eq!(r, "docs/images/x.png");
    }

    #[test]
    fn normalize_relative_strips_dot_slash() {
        let r = normalize_relative("README.md", "./CONTRIBUTING.md").unwrap();
        assert_eq!(r, "CONTRIBUTING.md");
    }
}
