use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

pub type EnvMap = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PipelineSource {
    GitHubActions,
    NativeToml,
    Api,
    Agent,
    MergeQueue,
    Hotfix,
    Release,
    Scheduled,
    Unknown(String),
}

impl PipelineSource {
    pub fn as_str(&self) -> &str {
        match self {
            Self::GitHubActions => "github-actions",
            Self::NativeToml => "jit-native",
            Self::Api => "api",
            Self::Agent => "agent",
            Self::MergeQueue => "merge-queue",
            Self::Hotfix => "hotfix",
            Self::Release => "release",
            Self::Scheduled => "scheduled",
            Self::Unknown(value) => value.as_str(),
        }
    }
}

impl fmt::Display for PipelineSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrustTier {
    ReleaseHermetic,
    ProtectedInternal,
    InternalBranch,
    AgentAuthored,
    ForkPr,
    PublicUntrusted,
}

impl TrustTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReleaseHermetic => "T0-release-hermetic",
            Self::ProtectedInternal => "T1-protected-internal",
            Self::InternalBranch => "T2-internal-branch",
            Self::AgentAuthored => "T3-agent-authored",
            Self::ForkPr => "T4-fork-pr",
            Self::PublicUntrusted => "T5-public-untrusted",
        }
    }
}

impl FromStr for TrustTier {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match normalize_token(value).as_str() {
            "t0releasehermetic" | "releasehermetic" | "release" => Ok(Self::ReleaseHermetic),
            "t1protectedinternal" | "protectedinternal" | "protected" => {
                Ok(Self::ProtectedInternal)
            }
            "t2internalbranch" | "internalbranch" | "internal" => Ok(Self::InternalBranch),
            "t3agentauthored" | "agentauthored" | "agent" => Ok(Self::AgentAuthored),
            "t4forkpr" | "forkpr" | "fork" => Ok(Self::ForkPr),
            "t5publicuntrusted" | "publicuntrusted" | "public" | "untrusted" => {
                Ok(Self::PublicUntrusted)
            }
            other => Err(format!("unknown trust tier: {other}")),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum RunnerClass {
    NativeRustHot,
    #[default]
    NativeRustClean,
    CrategraphDelta,
    NextestCapsule,
    AgentGuard,
    MergeSpec,
    ReleaseHermetic,
    MicrovmRust,
    OciDocker,
    K8sOci,
    Custom(String),
}

impl RunnerClass {
    pub fn as_str(&self) -> &str {
        match self {
            Self::NativeRustHot => "native-rust-hot",
            Self::NativeRustClean => "native-rust-clean",
            Self::CrategraphDelta => "crategraph-delta",
            Self::NextestCapsule => "nextest-capsule",
            Self::AgentGuard => "agent-guard",
            Self::MergeSpec => "merge-spec",
            Self::ReleaseHermetic => "release-hermetic",
            Self::MicrovmRust => "microvm-rust",
            Self::OciDocker => "oci-docker",
            Self::K8sOci => "k8s-oci",
            Self::Custom(value) => value.as_str(),
        }
    }
}

impl FromStr for RunnerClass {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = trim_quotes(value).trim();
        match normalize_token(trimmed).as_str() {
            "nativerusthot" => Ok(Self::NativeRustHot),
            "nativerustclean" | "ubuntulatest" | "linux" => Ok(Self::NativeRustClean),
            "crategraphdelta" => Ok(Self::CrategraphDelta),
            "nextestcapsule" => Ok(Self::NextestCapsule),
            "agentguard" => Ok(Self::AgentGuard),
            "mergespec" => Ok(Self::MergeSpec),
            "releasehermetic" => Ok(Self::ReleaseHermetic),
            "microvmrust" | "microvm" => Ok(Self::MicrovmRust),
            "ocidocker" | "docker" => Ok(Self::OciDocker),
            "k8soci" | "kubernetes" | "k8s" => Ok(Self::K8sOci),
            _ if !trimmed.is_empty() => Ok(Self::Custom(trimmed.to_string())),
            _ => Err("runner class cannot be empty".to_string()),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum NetworkPolicy {
    #[default]
    Deny,
    Allowlist(Vec<String>),
    Open,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TokenScope {
    None,
    #[default]
    ReadRepo,
    WriteChecks,
    WritePullRequest,
    Custom(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub backoff_seconds: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            backoff_seconds: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CacheMode {
    ReadOnly,
    ReadWriteQuarantine,
    ReadWriteTrusted,
}

impl CacheMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read-only",
            Self::ReadWriteQuarantine => "read-write-quarantine",
            Self::ReadWriteTrusted => "read-write-trusted",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheMount {
    pub name: String,
    pub path: String,
    pub mode: CacheMode,
    pub fingerprint: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArtifactWhen {
    Always,
    OnSuccess,
    OnFailure,
}

impl ArtifactWhen {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::OnSuccess => "on-success",
            Self::OnFailure => "on-failure",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactPath {
    pub name: String,
    pub paths: Vec<String>,
    pub when: ArtifactWhen,
    pub retention_days: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub id: String,
    pub name: String,
    pub command: Option<String>,
    pub uses: Option<String>,
    pub env: EnvMap,
    pub working_directory: Option<String>,
}

impl Step {
    pub fn run(id: impl Into<String>, name: impl Into<String>, command: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            command: Some(command.into()),
            uses: None,
            env: EnvMap::new(),
            working_directory: None,
        }
    }

    pub fn uses(id: impl Into<String>, name: impl Into<String>, action: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            command: None,
            uses: Some(action.into()),
            env: EnvMap::new(),
            working_directory: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub name: String,
    pub runner_class: RunnerClass,
    pub steps: Vec<Step>,
    pub inputs: BTreeMap<String, String>,
    pub outputs: BTreeMap<String, String>,
    pub cache_mounts: Vec<CacheMount>,
    pub artifact_paths: Vec<ArtifactPath>,
    pub network_policy: NetworkPolicy,
    pub token_scope: TokenScope,
    pub timeout_seconds: u64,
    pub retry_policy: RetryPolicy,
}

impl Job {
    pub fn new(id: impl Into<String>, name: impl Into<String>, runner_class: RunnerClass) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            runner_class,
            steps: Vec::new(),
            inputs: BTreeMap::new(),
            outputs: BTreeMap::new(),
            cache_mounts: Vec::new(),
            artifact_paths: Vec::new(),
            network_policy: NetworkPolicy::Deny,
            token_scope: TokenScope::ReadRepo,
            timeout_seconds: 3600,
            retry_policy: RetryPolicy::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachePolicy {
    pub project_scoped: bool,
    pub allow_cross_project_compiled: bool,
    pub promote_after_green: bool,
    pub quarantine_untrusted_writes: bool,
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self {
            project_scoped: true,
            allow_cross_project_compiled: false,
            promote_after_green: true,
            quarantine_untrusted_writes: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactPolicy {
    pub allow_absolute_paths: bool,
    pub require_metadata: bool,
    pub default_retention_days: u32,
}

impl Default for ArtifactPolicy {
    fn default() -> Self {
        Self {
            allow_absolute_paths: false,
            require_metadata: true,
            default_retention_days: 14,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PermissionPolicy {
    pub default_token_scope: TokenScope,
    pub fail_closed: bool,
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self {
            default_token_scope: TokenScope::ReadRepo,
            fail_closed: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SecretPolicy {
    pub secrets_available: bool,
    pub deny_on_fork: bool,
}

impl Default for SecretPolicy {
    fn default() -> Self {
        Self {
            secrets_available: false,
            deny_on_fork: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofPolicy {
    pub proof_required: bool,
    pub lane: String,
}

impl Default for ProofPolicy {
    fn default() -> Self {
        Self {
            proof_required: true,
            lane: "phase3-fast".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SigningPolicy {
    pub provenance_required: bool,
    pub release_only: bool,
}

impl Default for SigningPolicy {
    fn default() -> Self {
        Self {
            provenance_required: false,
            release_only: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pipeline {
    pub id: String,
    pub source: PipelineSource,
    pub repo: String,
    pub commit: String,
    pub trust_tier: TrustTier,
    pub jobs: Vec<Job>,
    pub edges: Vec<Dependency>,
    pub cache_policy: CachePolicy,
    pub artifact_policy: ArtifactPolicy,
    pub permission_policy: PermissionPolicy,
    pub secret_policy: SecretPolicy,
    pub proof_policy: ProofPolicy,
    pub signing_policy: SigningPolicy,
}

impl Pipeline {
    pub fn new(
        source: PipelineSource,
        repo: impl Into<String>,
        commit: impl Into<String>,
        trust_tier: TrustTier,
    ) -> Self {
        let repo = repo.into();
        let commit = commit.into();
        let id = deterministic_hash(&format!("pipeline|{}|{}|{}", source.as_str(), repo, commit));
        Self {
            id,
            source,
            repo,
            commit,
            trust_tier,
            jobs: Vec::new(),
            edges: Vec::new(),
            cache_policy: CachePolicy::default(),
            artifact_policy: ArtifactPolicy::default(),
            permission_policy: PermissionPolicy::default(),
            secret_policy: SecretPolicy::default(),
            proof_policy: ProofPolicy::default(),
            signing_policy: SigningPolicy::default(),
        }
    }

    pub fn canonical(&self) -> String {
        let mut out = String::new();
        line(&mut out, "pipeline.id", &self.id);
        line(&mut out, "pipeline.source", self.source.as_str());
        line(&mut out, "pipeline.repo", &self.repo);
        line(&mut out, "pipeline.commit", &self.commit);
        line(&mut out, "pipeline.trust_tier", self.trust_tier.as_str());
        line(
            &mut out,
            "cache.project_scoped",
            self.cache_policy.project_scoped,
        );
        line(
            &mut out,
            "cache.allow_cross_project_compiled",
            self.cache_policy.allow_cross_project_compiled,
        );
        line(
            &mut out,
            "cache.promote_after_green",
            self.cache_policy.promote_after_green,
        );
        line(
            &mut out,
            "cache.quarantine_untrusted_writes",
            self.cache_policy.quarantine_untrusted_writes,
        );
        line(
            &mut out,
            "artifact.allow_absolute_paths",
            self.artifact_policy.allow_absolute_paths,
        );
        line(
            &mut out,
            "artifact.require_metadata",
            self.artifact_policy.require_metadata,
        );
        line(
            &mut out,
            "artifact.default_retention_days",
            self.artifact_policy.default_retention_days,
        );
        line(
            &mut out,
            "permission.fail_closed",
            self.permission_policy.fail_closed,
        );
        line(
            &mut out,
            "secret.secrets_available",
            self.secret_policy.secrets_available,
        );
        line(
            &mut out,
            "secret.deny_on_fork",
            self.secret_policy.deny_on_fork,
        );
        line(&mut out, "proof.required", self.proof_policy.proof_required);
        line(&mut out, "proof.lane", &self.proof_policy.lane);
        line(
            &mut out,
            "signing.provenance_required",
            self.signing_policy.provenance_required,
        );
        line(
            &mut out,
            "signing.release_only",
            self.signing_policy.release_only,
        );

        let mut jobs = self.jobs.clone();
        jobs.sort_by(|a, b| a.id.cmp(&b.id));
        for job in jobs {
            job.canonical_into(&mut out);
        }

        let mut edges = self.edges.clone();
        edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
        for edge in edges {
            line(&mut out, "edge", format!("{}->{}", edge.from, edge.to));
        }
        out
    }

    pub fn ir_hash(&self) -> String {
        deterministic_hash(&self.canonical())
    }

    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.repo.trim().is_empty() {
            return Err(ValidationError::EmptyRepo);
        }
        if self.commit.trim().is_empty() {
            return Err(ValidationError::EmptyCommit);
        }
        let mut seen = BTreeSet::new();
        for job in &self.jobs {
            if job.id.trim().is_empty() {
                return Err(ValidationError::EmptyJobId);
            }
            if !seen.insert(job.id.clone()) {
                return Err(ValidationError::DuplicateJob(job.id.clone()));
            }
            if job.steps.is_empty() {
                return Err(ValidationError::JobHasNoSteps(job.id.clone()));
            }
        }
        for edge in &self.edges {
            if !seen.contains(&edge.from) {
                return Err(ValidationError::UnknownEdgeEndpoint(edge.from.clone()));
            }
            if !seen.contains(&edge.to) {
                return Err(ValidationError::UnknownEdgeEndpoint(edge.to.clone()));
            }
            if edge.from == edge.to {
                return Err(ValidationError::SelfDependency(edge.from.clone()));
            }
        }
        Ok(())
    }
}

impl Job {
    fn canonical_into(&self, out: &mut String) {
        line(out, "job.id", &self.id);
        line(out, "job.name", &self.name);
        line(out, "job.runner", self.runner_class.as_str());
        line(out, "job.timeout_seconds", self.timeout_seconds);
        line(
            out,
            "job.retry.max_attempts",
            self.retry_policy.max_attempts,
        );
        line(
            out,
            "job.retry.backoff_seconds",
            self.retry_policy.backoff_seconds,
        );
        line(out, "job.network", network_to_string(&self.network_policy));
        line(out, "job.token", token_to_string(&self.token_scope));
        for (key, value) in &self.inputs {
            line(out, format!("job.input.{key}"), value);
        }
        for (key, value) in &self.outputs {
            line(out, format!("job.output.{key}"), value);
        }
        let mut cache_mounts = self.cache_mounts.clone();
        cache_mounts.sort_by(|a, b| (&a.name, &a.path).cmp(&(&b.name, &b.path)));
        for mount in cache_mounts {
            line(
                out,
                "job.cache",
                format!(
                    "{}|{}|{}|{}",
                    mount.name,
                    mount.path,
                    mount.mode.as_str(),
                    mount.fingerprint
                ),
            );
        }
        let mut artifacts = self.artifact_paths.clone();
        artifacts.sort_by(|a, b| a.name.cmp(&b.name));
        for artifact in artifacts {
            line(
                out,
                "job.artifact",
                format!(
                    "{}|{}|{}|{}",
                    artifact.name,
                    artifact.when.as_str(),
                    artifact.retention_days,
                    artifact.paths.join(",")
                ),
            );
        }
        for step in &self.steps {
            line(out, "job.step.id", &step.id);
            line(out, "job.step.name", &step.name);
            if let Some(command) = &step.command {
                line(out, "job.step.run", command);
            }
            if let Some(uses) = &step.uses {
                line(out, "job.step.uses", uses);
            }
            if let Some(dir) = &step.working_directory {
                line(out, "job.step.cwd", dir);
            }
            for (key, value) in &step.env {
                line(out, format!("job.step.env.{key}"), value);
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    EmptyRepo,
    EmptyCommit,
    EmptyJobId,
    DuplicateJob(String),
    JobHasNoSteps(String),
    UnknownEdgeEndpoint(String),
    SelfDependency(String),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRepo => f.write_str("pipeline repo cannot be empty"),
            Self::EmptyCommit => f.write_str("pipeline commit cannot be empty"),
            Self::EmptyJobId => f.write_str("job id cannot be empty"),
            Self::DuplicateJob(job) => write!(f, "duplicate job id: {job}"),
            Self::JobHasNoSteps(job) => write!(f, "job has no steps: {job}"),
            Self::UnknownEdgeEndpoint(job) => write!(f, "edge references unknown job: {job}"),
            Self::SelfDependency(job) => write!(f, "job cannot depend on itself: {job}"),
        }
    }
}

impl std::error::Error for ValidationError {}

pub fn deterministic_hash(input: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("fnv64:{hash:016x}")
}

pub fn stable_id(prefix: &str, value: &str) -> String {
    let hash = deterministic_hash(value).replace("fnv64:", "");
    format!("{prefix}_{hash}")
}

pub fn sanitize_id(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '_' || ch == '-' || ch == '.' {
            out.push(ch);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed
    }
}

pub fn trim_quotes(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
        {
            return &trimmed[1..trimmed.len() - 1];
        }
    }
    trimmed
}

fn normalize_token(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn line<K: fmt::Display, V: fmt::Display>(out: &mut String, key: K, value: V) {
    out.push_str(&key.to_string());
    out.push('=');
    out.push_str(&escape_canonical(&value.to_string()));
    out.push('\n');
}

fn escape_canonical(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\n', "\\n")
}

fn network_to_string(policy: &NetworkPolicy) -> String {
    match policy {
        NetworkPolicy::Deny => "deny".to_string(),
        NetworkPolicy::Allowlist(hosts) => format!("allowlist:{}", hosts.join(",")),
        NetworkPolicy::Open => "open".to_string(),
    }
}

fn token_to_string(scope: &TokenScope) -> String {
    match scope {
        TokenScope::None => "none".to_string(),
        TokenScope::ReadRepo => "read-repo".to_string(),
        TokenScope::WriteChecks => "write-checks".to_string(),
        TokenScope::WritePullRequest => "write-pull-request".to_string(),
        TokenScope::Custom(values) => format!("custom:{}", values.join(",")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pipeline() -> Pipeline {
        let mut pipeline = Pipeline::new(
            PipelineSource::NativeToml,
            "acme/demo",
            "abc123",
            TrustTier::InternalBranch,
        );
        let mut fmt = Job::new("fmt", "fmt", RunnerClass::NativeRustClean);
        fmt.steps
            .push(Step::run("fmt_0", "fmt", "cargo fmt --check"));
        let mut test = Job::new("test", "test", RunnerClass::NativeRustClean);
        test.steps.push(Step::run("test_0", "test", "cargo test"));
        pipeline.jobs.push(test);
        pipeline.jobs.push(fmt);
        pipeline.edges.push(Dependency {
            from: "fmt".to_string(),
            to: "test".to_string(),
        });
        pipeline
    }

    #[test]
    fn hash_is_stable_for_same_pipeline() -> Result<(), Box<dyn std::error::Error>> {
        let a = sample_pipeline();
        let b = sample_pipeline();
        assert_eq!(a.ir_hash(), b.ir_hash());
        assert_eq!(a.canonical(), b.canonical());
        Ok(())
    }

    #[test]
    fn validation_rejects_unknown_edges() {
        let mut pipeline = sample_pipeline();
        pipeline.edges.push(Dependency {
            from: "missing".to_string(),
            to: "fmt".to_string(),
        });
        assert!(matches!(
            pipeline.validate(),
            Err(ValidationError::UnknownEdgeEndpoint(_))
        ));
    }

    #[test]
    fn sanitize_keeps_deterministic_ids() {
        assert_eq!(sanitize_id("Rust stable / Ubuntu"), "rust_stable_ubuntu");
        assert_eq!(sanitize_id("!!!"), "unnamed");
    }
}
