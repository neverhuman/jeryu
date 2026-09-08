use std::path::{Path, PathBuf};

pub fn normalize_slashes(path: impl AsRef<Path>) -> String {
    let mut out = String::new();
    for component in path.as_ref().components() {
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&component.as_os_str().to_string_lossy());
    }
    out
}

pub fn normalize_relative(root: &Path, path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let rel = path.strip_prefix(root).unwrap_or(path);
    normalize_slashes(rel)
}

pub fn join(root: &Path, rel: &str) -> PathBuf {
    rel.split('/').fold(root.to_path_buf(), |mut acc, part| {
        acc.push(part);
        acc
    })
}

pub fn is_markdown(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.ends_with(".md") || p.ends_with(".markdown") || p.ends_with(".rst") || p.starts_with("docs/")
}

pub fn is_security_sensitive(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("/security/")
        || p.contains("/crypto/")
        || p.starts_with("security/")
        || p.starts_with("ops/ci/")
        || p.starts_with(".github/")
        || p.starts_with("agent/")
}
