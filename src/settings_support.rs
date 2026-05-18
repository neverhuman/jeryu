use super::*;

/// Resolve the release repository root with explicit settings precedence.
pub fn release_repo_root() -> PathBuf {
    if let Ok(value) = std::env::var("JERYU_RELEASE_REPO_ROOT")
        && !value.trim().is_empty()
    {
        return PathBuf::from(value);
    }
    if let Some(value) = get()
        .release
        .repo_root
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        return PathBuf::from(value);
    }
    PathBuf::from("/home/ubuntu/dougx")
}
