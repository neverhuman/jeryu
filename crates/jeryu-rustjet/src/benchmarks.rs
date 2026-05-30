#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkExpectation {
    pub name: String,
    pub max_duration_ms: u64,
    pub min_speedup_vs_baseline_container: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BenchmarkResult {
    pub name: String,
    pub duration_ms: u64,
    pub speedup_vs_baseline_container: f64,
    pub false_cache_hits: u64,
}

impl BenchmarkExpectation {
    #[must_use]
    pub fn warm_medium_pr() -> Self {
        Self {
            name: "warm-medium-rust-pr".to_string(),
            max_duration_ms: 60_000,
            min_speedup_vs_baseline_container: 3.0,
        }
    }

    #[must_use]
    pub fn evaluate(&self, result: &BenchmarkResult) -> bool {
        result.name == self.name
            && result.duration_ms <= self.max_duration_ms
            && result.speedup_vs_baseline_container >= self.min_speedup_vs_baseline_container
            && result.false_cache_hits == 0
    }
}
