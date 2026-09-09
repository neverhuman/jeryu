//! Release store selection. Bundled SQLite is the durable default.
//!
//! Redline is an optional SQL engine and is not linked into this binary.
//! Requesting it does not fail serve and does not block the SQLite release.

/// Durable backend this release binary actually opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStore {
    /// Bundled SQLite (the only linked engine in this release).
    Sqlite,
}

/// Parse `--store` / `JERYU_STORE`. Empty or omitted means SQLite.
pub fn resolve(explicit: Option<&str>) -> Result<ResolvedStore, String> {
    let raw = explicit.unwrap_or("").trim();
    if raw.is_empty() {
        return Ok(ResolvedStore {
            requested: "sqlite",
            runtime: RuntimeStore::Sqlite,
            fallback: false,
        });
    }
    match raw.to_ascii_lowercase().as_str() {
        "sqlite" => Ok(ResolvedStore {
            requested: "sqlite",
            runtime: RuntimeStore::Sqlite,
            fallback: false,
        }),
        "redline" | "redlinedb" => Ok(ResolvedStore {
            requested: "redline",
            runtime: RuntimeStore::Sqlite,
            fallback: true,
        }),
        other => Err(format!(
            "unknown store {other:?}; use sqlite (default) or redline"
        )),
    }
}

/// Result of store selection. `fallback` is true when Redline was requested
/// and this binary continues on bundled SQLite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedStore {
    /// Operator-requested name.
    pub requested: &'static str,
    /// Engine this process will open.
    pub runtime: RuntimeStore,
    /// True when Redline was requested and SQLite is used instead.
    pub fallback: bool,
}

impl ResolvedStore {
    /// Operator-facing notice when Redline is requested on this binary.
    pub fn fallback_notice(self) -> Option<&'static str> {
        self.fallback.then_some(
            "store=redline is optional; this release binary uses bundled SQLite and continues",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sqlite_is_default_and_redline_falls_back_without_blocking() {
        assert_eq!(
            resolve(None).unwrap(),
            ResolvedStore {
                requested: "sqlite",
                runtime: RuntimeStore::Sqlite,
                fallback: false,
            }
        );
        assert_eq!(
            resolve(Some("sqlite")).unwrap().runtime,
            RuntimeStore::Sqlite
        );
        assert!(!resolve(Some("sqlite")).unwrap().fallback);
        let redline = resolve(Some("RedlineDB")).unwrap();
        assert_eq!(redline.requested, "redline");
        assert_eq!(redline.runtime, RuntimeStore::Sqlite);
        assert!(redline.fallback);
        assert!(redline.fallback_notice().is_some());
        assert!(resolve(Some("postgres")).is_err());
    }
}
