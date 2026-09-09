use jeryu_readmodel::contracts::{MarkdownHeading, MarkdownLink, RenderedMarkdown};

pub(super) fn render_markdown(markdown: &str) -> RenderedMarkdown {
    let mut html = String::new();
    let mut toc = Vec::new();
    for line in markdown.lines() {
        if let Some(title) = line.strip_prefix("# ") {
            let id = slug(title);
            html.push_str(&format!("<h1 id=\"{id}\">{}</h1>", escape(title)));
            toc.push(MarkdownHeading {
                depth: 1,
                id,
                text: title.to_string(),
            });
        } else if line.trim().is_empty() {
            continue;
        } else {
            html.push_str(&format!("<p>{}</p>", escape(line)));
        }
    }
    RenderedMarkdown {
        html,
        toc,
        links: Vec::<MarkdownLink>::new(),
        renderer_version: "jeryu-md-renderer.v1".to_string(),
        sanitizer_version: Some("jeryu-md-sanitizer.v1".to_string()),
        rendered_at: super::server_time(),
    }
}

fn slug(value: &str) -> String {
    let slug = value
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug
    }
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
