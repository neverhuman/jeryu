//! Public benchmark receipts.

use crate::models::{
    CacheState, Competitor, JeryuRunner, ScenarioClass, TrustTier, json_escape, stable_digest,
};

/// Receipt validation failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReceiptError {
    MissingHardware,
    MissingGitSha,
    MissingPipelineHash,
    MissingArtifactDigest,
    NonPositiveDuration,
    NegativeFalseCacheHits,
    MissingReplayCommand,
}

/// Reproducible receipt for a single benchmark comparison.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchmarkReceipt {
    pub benchmark_id: String,
    pub competitor: Competitor,
    pub jeryu_runner: JeryuRunner,
    pub repo_fixture: String,
    pub scenario: ScenarioClass,
    pub hardware: String,
    pub cache_state: CacheState,
    pub trust_tier: TrustTier,
    pub git_sha: String,
    pub pipeline_ir_hash: String,
    pub competitor_duration_ms: u64,
    pub jeryu_duration_ms: u64,
    pub speedup_vs_competitor: f64,
    pub false_cache_hits: u32,
    pub artifact_digest: String,
    pub reproduce: String,
}

impl BenchmarkReceipt {
    /// Build a deterministic receipt from the core dimensions.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        competitor: Competitor,
        jeryu_runner: JeryuRunner,
        repo_fixture: impl Into<String>,
        scenario: ScenarioClass,
        hardware: impl Into<String>,
        cache_state: CacheState,
        trust_tier: TrustTier,
        git_sha: impl Into<String>,
        pipeline_ir_hash: impl Into<String>,
        competitor_duration_ms: u64,
        jeryu_duration_ms: u64,
        false_cache_hits: u32,
    ) -> Self {
        let repo_fixture = repo_fixture.into();
        let hardware = hardware.into();
        let git_sha = git_sha.into();
        let pipeline_ir_hash = pipeline_ir_hash.into();
        let speedup_vs_competitor = if jeryu_duration_ms == 0 {
            0.0
        } else {
            competitor_duration_ms as f64 / jeryu_duration_ms as f64
        };
        let benchmark_id = stable_digest(&[
            competitor.slug(),
            jeryu_runner.slug(),
            &repo_fixture,
            scenario.slug(),
            &hardware,
            cache_state.slug(),
            trust_tier.slug(),
            &git_sha,
            &pipeline_ir_hash,
        ]);
        let artifact_digest = stable_digest(&[
            "artifact",
            &benchmark_id,
            &competitor_duration_ms.to_string(),
            &jeryu_duration_ms.to_string(),
        ]);
        let reproduce = format!("jit bench replay {benchmark_id}");
        Self {
            benchmark_id,
            competitor,
            jeryu_runner,
            repo_fixture,
            scenario,
            hardware,
            cache_state,
            trust_tier,
            git_sha,
            pipeline_ir_hash,
            competitor_duration_ms,
            jeryu_duration_ms,
            speedup_vs_competitor,
            false_cache_hits,
            artifact_digest,
            reproduce,
        }
    }

    /// Validate required public evidence fields.
    pub fn validate(&self) -> Result<(), ReceiptError> {
        if self.hardware.trim().is_empty() {
            return Err(ReceiptError::MissingHardware);
        }
        if self.git_sha.trim().is_empty() {
            return Err(ReceiptError::MissingGitSha);
        }
        if self.pipeline_ir_hash.trim().is_empty() {
            return Err(ReceiptError::MissingPipelineHash);
        }
        if self.artifact_digest.trim().is_empty() {
            return Err(ReceiptError::MissingArtifactDigest);
        }
        if self.competitor_duration_ms == 0 || self.jeryu_duration_ms == 0 {
            return Err(ReceiptError::NonPositiveDuration);
        }
        if !self.reproduce.starts_with("jit bench replay ") {
            return Err(ReceiptError::MissingReplayCommand);
        }
        Ok(())
    }

    /// True when the benchmark proves cache correctness for this run.
    pub const fn cache_safe(&self) -> bool {
        self.false_cache_hits == 0
    }

    /// Emit stable JSON for publication.
    pub fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"benchmark_id\": \"{}\",\n",
                "  \"competitor\": \"{}\",\n",
                "  \"jeryu_runner\": \"{}\",\n",
                "  \"repo_fixture\": \"{}\",\n",
                "  \"scenario\": \"{}\",\n",
                "  \"hardware\": \"{}\",\n",
                "  \"cache_state\": \"{}\",\n",
                "  \"trust_tier\": \"{}\",\n",
                "  \"git_sha\": \"{}\",\n",
                "  \"pipeline_ir_hash\": \"{}\",\n",
                "  \"competitor_duration_ms\": {},\n",
                "  \"jeryu_duration_ms\": {},\n",
                "  \"speedup_vs_competitor\": {:.3},\n",
                "  \"false_cache_hits\": {},\n",
                "  \"artifact_digest\": \"{}\",\n",
                "  \"reproduce\": \"{}\"\n",
                "}}"
            ),
            json_escape(&self.benchmark_id),
            self.competitor.slug(),
            self.jeryu_runner.slug(),
            json_escape(&self.repo_fixture),
            self.scenario.slug(),
            json_escape(&self.hardware),
            self.cache_state.slug(),
            self.trust_tier.slug(),
            json_escape(&self.git_sha),
            json_escape(&self.pipeline_ir_hash),
            self.competitor_duration_ms,
            self.jeryu_duration_ms,
            self.speedup_vs_competitor,
            self.false_cache_hits,
            json_escape(&self.artifact_digest),
            json_escape(&self.reproduce),
        )
    }
}
