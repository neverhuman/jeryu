use crate::manifest::WorkspacePackage;
use crate::pathset::join;
use std::fs;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicApiChange {
    pub package: String,
    pub path: String,
    pub symbols: Vec<String>,
    pub conservative: bool,
}

#[derive(Debug, Default, Clone)]
pub struct PublicApiDetector;

impl PublicApiDetector {
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    #[must_use]
    pub fn detect(
        &self,
        package: &WorkspacePackage,
        relative_inside_package: &str,
    ) -> Option<PublicApiChange> {
        if !looks_like_rust_source(relative_inside_package) {
            return None;
        }

        let conservative = is_public_surface_path(relative_inside_package);
        let absolute = join(&package.root, relative_inside_package);
        // Keep the read failure branch explicit instead of `.unwrap_or_default()`
        // so the error-hiding path stays auditable; the comment below documents
        // why an unreadable file is intentionally treated as empty source.
        #[allow(clippy::manual_unwrap_or_default)]
        let content = match fs::read_to_string(&absolute) {
            Ok(content) => content,
            // The file under analysis is unreadable (e.g. it was deleted in the
            // change set, or is not yet materialized on disk). Treat it as having
            // no source so symbol extraction finds nothing; the `conservative`
            // flag for public-surface paths still forces detection on its own.
            Err(_) => String::new(),
        };
        let symbols = public_symbols(&content);

        if conservative || !symbols.is_empty() {
            return Some(PublicApiChange {
                package: package.name.clone(),
                path: relative_inside_package.to_string(),
                symbols,
                conservative,
            });
        }
        None
    }
}

fn looks_like_rust_source(path: &str) -> bool {
    path.ends_with(".rs")
}

fn is_public_surface_path(path: &str) -> bool {
    path == "src/lib.rs"
        || path == "src/main.rs"
        || path.starts_with("src/api")
        || path.starts_with("src/public")
        || path.starts_with("src/protocol")
}

fn public_symbols(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("pub ")
            || trimmed.starts_with("pub(crate) ")
            || trimmed.starts_with("pub(super) "))
        {
            continue;
        }
        if trimmed.starts_with("pub(crate)") || trimmed.starts_with("pub(super)") {
            continue;
        }
        let words: Vec<_> = trimmed
            .split(|ch: char| {
                ch.is_whitespace() || ch == '(' || ch == '<' || ch == '{' || ch == ';'
            })
            .filter(|part| !part.is_empty())
            .collect();
        for window in words.windows(2) {
            if matches!(
                window[0],
                "fn" | "struct" | "enum" | "trait" | "type" | "const" | "static" | "mod"
            ) {
                out.push(window[1].to_string());
                break;
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::public_symbols;

    #[test]
    fn finds_public_symbols_only() {
        let symbols =
            public_symbols("pub fn api() {}\npub(crate) fn hidden() {}\npub struct User;\n");
        assert_eq!(symbols, vec!["User", "api"]);
    }
}
