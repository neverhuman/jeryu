//! Owner: Web Forge BFF — in-memory markdown render cache (W-B-08, Phase 1).
//! Proof: `cargo nextest run -p jeryu --lib repo_browser::render_cache`
//! Invariants: Cache key includes BOTH `renderer_version` and
//! `sanitizer_version` per §35.1.4 so either constant can be bumped
//! independently to invalidate stale entries.
//!
//! Phase 2 (separate work package) replaces this with a SQLite-backed
//! `web_markdown_cache` table.

use std::collections::HashMap;

use parking_lot::Mutex;

use crate::api::repo_browser::RenderedMarkdown;

#[derive(Hash, PartialEq, Eq, Clone, Debug)]
pub struct CacheKey {
    pub repo_id: String,
    pub ref_sha: String,
    pub path: String,
    pub blob_sha: String,
    pub renderer_version: String,
    pub sanitizer_version: String,
}

pub struct MarkdownCache {
    inner: Mutex<HashMap<CacheKey, RenderedMarkdown>>,
    max_entries: usize,
}

impl MarkdownCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
            max_entries,
        }
    }

    pub fn get(&self, key: &CacheKey) -> Option<RenderedMarkdown> {
        self.inner.lock().get(key).cloned()
    }

    pub fn put(&self, key: CacheKey, value: RenderedMarkdown) {
        let mut g = self.inner.lock();
        if g.len() >= self.max_entries {
            // Evict an arbitrary entry. Phase 2 will use a proper LRU.
            if let Some(k) = g.keys().next().cloned() {
                g.remove(&k);
            }
        }
        g.insert(key, value);
    }

    pub fn len(&self) -> usize {
        self.inner.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn sample() -> RenderedMarkdown {
        RenderedMarkdown {
            html: "<p>hi</p>".into(),
            toc: vec![],
            links: vec![],
            renderer_version: super::super::markdown::RENDERER_VERSION.into(),
            sanitizer_version: Some(super::super::markdown::SANITIZER_VERSION.into()),
            rendered_at: Utc::now(),
        }
    }

    fn key(suffix: &str) -> CacheKey {
        CacheKey {
            repo_id: "r".into(),
            ref_sha: "s".into(),
            path: format!("p{suffix}"),
            blob_sha: "b".into(),
            renderer_version: super::super::markdown::RENDERER_VERSION.into(),
            sanitizer_version: super::super::markdown::SANITIZER_VERSION.into(),
        }
    }

    #[test]
    fn put_and_get_roundtrip() {
        let cache = MarkdownCache::new(8);
        cache.put(key("a"), sample());
        assert!(cache.get(&key("a")).is_some());
        assert!(cache.get(&key("b")).is_none());
    }

    #[test]
    fn cache_evicts_when_full() {
        let cache = MarkdownCache::new(2);
        cache.put(key("a"), sample());
        cache.put(key("b"), sample());
        cache.put(key("c"), sample());
        assert!(cache.len() <= 2);
    }
}
