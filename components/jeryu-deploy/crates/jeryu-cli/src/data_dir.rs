//! Durable standalone storage selection, independent of the current directory.

use std::path::PathBuf;

/// Resolve --data-dir, JERYU_DATA_DIR, XDG_DATA_HOME/jeryu, then HOME.
pub fn resolve(explicit: Option<PathBuf>) -> std::io::Result<PathBuf> {
    resolve_with(explicit, |name| std::env::var_os(name).map(PathBuf::from))
}

fn resolve_with(
    explicit: Option<PathBuf>,
    env: impl Fn(&str) -> Option<PathBuf>,
) -> std::io::Result<PathBuf> {
    if let Some(path) = explicit.or_else(|| env("JERYU_DATA_DIR")) {
        if path.as_os_str().is_empty() {
            return Err(std::io::Error::other("data directory must not be empty"));
        }
        if let Ok(rest) = path.strip_prefix("~") {
            return env("HOME").map(|home| home.join(rest)).ok_or_else(|| {
                std::io::Error::other("HOME is required to expand the data directory")
            });
        }
        return std::path::absolute(path);
    }
    if let Some(xdg) = env("XDG_DATA_HOME").filter(|path| path.is_absolute()) {
        return Ok(xdg.join("jeryu"));
    }
    env("HOME")
        .filter(|path| path.is_absolute())
        .map(|home| home.join(".local/share/jeryu"))
        .ok_or_else(|| {
            std::io::Error::other("set --data-dir, JERYU_DATA_DIR, XDG_DATA_HOME, or HOME")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durable_precedence_and_xdg_rules() {
        let env = |key: &str| match key {
            "JERYU_DATA_DIR" => Some(PathBuf::from("/durable")),
            "XDG_DATA_HOME" => Some(PathBuf::from("/xdg")),
            "HOME" => Some(PathBuf::from("/home/test")),
            _ => None,
        };
        assert_eq!(
            resolve_with(Some("/explicit".into()), env).unwrap(),
            PathBuf::from("/explicit")
        );
        assert_eq!(resolve_with(None, env).unwrap(), PathBuf::from("/durable"));
        assert_eq!(
            resolve_with(None, |key| if key == "JERYU_DATA_DIR" {
                None
            } else {
                env(key)
            })
            .unwrap(),
            PathBuf::from("/xdg/jeryu")
        );
        assert_eq!(
            resolve_with(None, |key| match key {
                "XDG_DATA_HOME" => Some("relative".into()),
                "HOME" => Some("/home/test".into()),
                _ => None,
            })
            .unwrap(),
            PathBuf::from("/home/test/.local/share/jeryu")
        );
        assert!(resolve_with(None, |_| None).is_err());
        assert!(resolve_with(Some(PathBuf::new()), env).is_err());
        assert_eq!(
            resolve_with(Some("~/forge".into()), env).unwrap(),
            PathBuf::from("/home/test/forge")
        );
    }
}
