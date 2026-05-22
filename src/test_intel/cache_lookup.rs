use super::*;

/// Given a set of tests and their cache keys, check which have cached verdicts.
///
/// In production, this would query the `cache_verdicts` table. Here we provide
/// the lookup logic that consumers (engine, test_runner) use.
pub fn check_cache(
    tests: &[(String, TestCacheKey)],
    cached_verdicts: &[CachedVerdict],
) -> CacheLookupResult {
    let mut hits = Vec::new();
    let mut misses = Vec::new();
    let mut time_saved_ms = 0u64;

    for (test_id, key) in tests {
        // Uncacheable tests are always misses
        if key.cacheability != Cacheability::Cacheable {
            misses.push(CacheMiss {
                test_id: test_id.clone(),
                reason: match key.uncacheable_reasons.first().cloned() {
                    Some(reason) => reason,
                    None => "uncacheable".into(),
                },
            });
            continue;
        }

        // Look for a matching verdict
        if let Some(verdict) = cached_verdicts
            .iter()
            .find(|v| v.cache_key == key.digest && v.passed)
        {
            hits.push(CacheHit {
                test_id: test_id.clone(),
                cache_key: key.digest.clone(),
                original_duration_ms: verdict.duration_ms,
                cached_at: verdict.cached_at.clone(),
            });
            time_saved_ms += verdict.duration_ms;
        } else {
            misses.push(CacheMiss {
                test_id: test_id.clone(),
                reason: "no cache hit".into(),
            });
        }
    }

    let total = tests.len().max(1);
    let hit_rate = hits.len() as f64 / total as f64;

    CacheLookupResult {
        hits,
        misses,
        time_saved_ms,
        hit_rate,
    }
}

/// Human-readable cache lookup report.
pub fn explain_cache_lookup(result: &CacheLookupResult) -> String {
    let mut out = String::new();
    out.push_str("╭─ VTI Test Cache Lookup ───────────────────────╮\n");
    out.push_str(&format!("│ Hits:       {:<34} │\n", result.hits.len()));
    out.push_str(&format!("│ Misses:     {:<34} │\n", result.misses.len()));
    out.push_str(&format!(
        "│ Hit rate:   {:<34.1}% │\n",
        result.hit_rate * 100.0
    ));
    out.push_str(&format!(
        "│ Time saved: {:<34} │\n",
        format_duration_ms(result.time_saved_ms)
    ));
    out.push_str("╰───────────────────────────────────────────────╯\n\n");

    if !result.hits.is_empty() {
        out.push_str("Cache hits (skippable):\n");
        for hit in &result.hits {
            out.push_str(&format!(
                "  ✓ {} (saved {})\n",
                hit.test_id,
                format_duration_ms(hit.original_duration_ms)
            ));
        }
        out.push('\n');
    }

    if !result.misses.is_empty() {
        out.push_str("Cache misses (must run):\n");
        for miss in &result.misses {
            out.push_str(&format!("  ● {} ({})\n", miss.test_id, miss.reason));
        }
    }

    out
}

/// JSON representation.
pub fn explain_cache_json(result: &CacheLookupResult) -> serde_json::Value {
    serde_json::json!({
        "hits": result.hits,
        "misses": result.misses,
        "time_saved_ms": result.time_saved_ms,
        "hit_rate": result.hit_rate,
    })
}

pub(crate) fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{:.1}m", ms as f64 / 60_000.0)
    }
}
