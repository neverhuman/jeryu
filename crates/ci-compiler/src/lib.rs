use ci_ir::{
    deterministic_hash, sanitize_id, trim_quotes, ArtifactPath, ArtifactWhen, CacheMode,
    CacheMount, Dependency, Job, Pipeline, PipelineSource, RunnerClass, Step, TokenScope,
    TrustTier,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CiKind {
    GitHubActions,
    NativeToml,
}

#[derive(Clone, Debug)]
pub struct CompileContext {
    pub repo: String,
    pub commit: String,
    pub trust_tier: TrustTier,
    pub default_runner: RunnerClass,
}

impl CompileContext {
    pub fn new(repo: impl Into<String>, commit: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            commit: commit.into(),
            trust_tier: TrustTier::InternalBranch,
            default_runner: RunnerClass::NativeRustClean,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError {
    EmptyInput,
    MissingJobs,
    MissingSteps(String),
    InvalidLine(String),
    InvalidRunner(String),
    InvalidDependency(String),
    Validation(String),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => f.write_str("CI input is empty"),
            Self::MissingJobs => f.write_str("CI input did not define any jobs"),
            Self::MissingSteps(job) => write!(f, "CI job did not define executable steps: {job}"),
            Self::InvalidLine(line) => write!(f, "invalid CI line: {line}"),
            Self::InvalidRunner(runner) => write!(f, "invalid runner class: {runner}"),
            Self::InvalidDependency(dep) => write!(f, "invalid dependency: {dep}"),
            Self::Validation(error) => write!(f, "IR validation failed: {error}"),
        }
    }
}

impl std::error::Error for CompileError {}

#[derive(Default)]
pub struct Compiler;

impl Compiler {
    pub fn compile(
        input: &str,
        kind: CiKind,
        context: CompileContext,
    ) -> Result<Pipeline, CompileError> {
        if input.trim().is_empty() {
            return Err(CompileError::EmptyInput);
        }
        let mut pipeline = match kind {
            CiKind::GitHubActions => compile_github(input, &context)?,
            CiKind::NativeToml => compile_native(input, &context)?,
        };
        pipeline.id = deterministic_hash(&format!(
            "pipeline|{}|{}|{}|{}",
            pipeline.source.as_str(),
            pipeline.repo,
            pipeline.commit,
            pipeline.ir_hash()
        ));
        pipeline
            .validate()
            .map_err(|err| CompileError::Validation(err.to_string()))?;
        Ok(pipeline)
    }
}

#[derive(Clone, Debug)]
struct SourceLine {
    indent: usize,
    text: String,
}

#[derive(Clone, Debug)]
struct PendingJob {
    origin: String,
    job: Job,
    needs: Vec<String>,
}

#[derive(Default)]
struct StepBuilder {
    name: Option<String>,
    run: Option<String>,
    uses: Option<String>,
    env: BTreeMap<String, String>,
    working_directory: Option<String>,
}

fn compile_github(input: &str, context: &CompileContext) -> Result<Pipeline, CompileError> {
    let lines = collect_lines(input);
    let jobs_index = find_line(&lines, 0, "jobs:").ok_or(CompileError::MissingJobs)?;
    let mut pending = Vec::new();
    let mut i = jobs_index + 1;
    while i < lines.len() {
        if lines[i].indent == 2 && is_yaml_map_header(&lines[i].text) {
            let origin = header_name(&lines[i].text);
            let mut j = i + 1;
            while j < lines.len() && !(lines[j].indent == 2 && is_yaml_map_header(&lines[j].text)) {
                j += 1;
            }
            pending.extend(parse_github_job(&origin, &lines[i + 1..j], context)?);
            i = j;
        } else {
            i += 1;
        }
    }
    finish_pipeline(PipelineSource::GitHubActions, pending, context)
}

fn parse_github_job(
    origin: &str,
    block: &[SourceLine],
    context: &CompileContext,
) -> Result<Vec<PendingJob>, CompileError> {
    let mut runner = context.default_runner.clone();
    let mut needs = Vec::new();
    let mut timeout_seconds = 3600;
    let mut steps = Vec::new();
    let mut matrix: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut token_scope = TokenScope::ReadRepo;
    let mut cache_mounts = Vec::new();
    let mut artifact_paths = Vec::new();

    let mut i = 0;
    while i < block.len() {
        let line = &block[i];
        if line.indent == 4 {
            if let Some(value) = scalar_after(&line.text, "runs-on:") {
                runner = RunnerClass::from_str(value)
                    .map_err(|_| CompileError::InvalidRunner(value.to_string()))?;
            } else if let Some(value) = scalar_after(&line.text, "needs:") {
                needs = parse_array_or_scalar(value);
            } else if line.text == "needs:" {
                let (slice, next) = nested_slice(block, i + 1, line.indent);
                needs = parse_yaml_list(slice);
                i = next;
                continue;
            } else if let Some(value) = scalar_after(&line.text, "timeout-minutes:") {
                timeout_seconds = value.trim().parse::<u64>().unwrap_or(60).saturating_mul(60);
            } else if let Some(value) = scalar_after(&line.text, "permissions:") {
                token_scope = parse_token_scope(value);
            } else if line.text == "steps:" {
                let (slice, next) = nested_slice(block, i + 1, line.indent);
                steps = parse_steps(origin, slice)?;
                i = next;
                continue;
            } else if line.text == "strategy:" {
                let (slice, next) = nested_slice(block, i + 1, line.indent);
                matrix = parse_matrix(slice);
                i = next;
                continue;
            } else if line.text == "cache:" {
                let (slice, next) = nested_slice(block, i + 1, line.indent);
                cache_mounts.extend(parse_cache_mounts(slice, origin));
                i = next;
                continue;
            } else if line.text == "artifacts:" {
                let (slice, next) = nested_slice(block, i + 1, line.indent);
                artifact_paths.extend(parse_artifacts(slice, origin));
                i = next;
                continue;
            }
        }
        i += 1;
    }

    if steps.is_empty() {
        return Err(CompileError::MissingSteps(origin.to_string()));
    }

    let combos = matrix_combinations(&matrix);
    let mut jobs = Vec::new();
    for combo in combos {
        let id = expanded_id(origin, &combo);
        let mut job = Job::new(&id, expanded_name(origin, &combo), runner.clone());
        job.steps = steps_with_matrix(&steps, &combo, &id);
        job.inputs = combo
            .iter()
            .map(|(k, v)| (format!("matrix.{k}"), v.clone()))
            .collect();
        job.timeout_seconds = timeout_seconds;
        job.token_scope = token_scope.clone();
        job.cache_mounts = if cache_mounts.is_empty() {
            default_cache_mounts(&id)
        } else {
            cache_mounts.clone()
        };
        job.artifact_paths = artifact_paths.clone();
        jobs.push(PendingJob {
            origin: origin.to_string(),
            job,
            needs: needs.clone(),
        });
    }
    Ok(jobs)
}

fn compile_native(input: &str, context: &CompileContext) -> Result<Pipeline, CompileError> {
    let mut pending = Vec::new();
    let mut current: BTreeMap<String, String> = BTreeMap::new();
    let mut current_arrays: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut in_job = false;

    for raw in input.lines() {
        let line = strip_comment(raw).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if line == "[[job]]" || line == "[[jobs]]" {
            if in_job {
                pending.push(native_job_from_maps(&current, &current_arrays, context)?);
                current.clear();
                current_arrays.clear();
            }
            in_job = true;
            continue;
        }
        if !in_job {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim().to_string();
            let value = value.trim();
            if value.starts_with('[') {
                current_arrays.insert(key, parse_array_or_scalar(value));
            } else {
                current.insert(key, trim_quotes(value).to_string());
            }
        } else {
            return Err(CompileError::InvalidLine(line));
        }
    }
    if in_job {
        pending.push(native_job_from_maps(&current, &current_arrays, context)?);
    }
    finish_pipeline(PipelineSource::NativeToml, pending, context)
}

fn native_job_from_maps(
    values: &BTreeMap<String, String>,
    arrays: &BTreeMap<String, Vec<String>>,
    context: &CompileContext,
) -> Result<PendingJob, CompileError> {
    let id = values
        .get("id")
        .cloned()
        .unwrap_or_else(|| "job".to_string());
    let name = values.get("name").cloned().unwrap_or_else(|| id.clone());
    let runner_text = values
        .get("runner_class")
        .or_else(|| values.get("runner"))
        .cloned()
        .unwrap_or_else(|| context.default_runner.as_str().to_string());
    let runner = RunnerClass::from_str(&runner_text)
        .map_err(|_| CompileError::InvalidRunner(runner_text.clone()))?;
    let mut job = Job::new(&id, name, runner);
    let run = arrays
        .get("run")
        .cloned()
        .or_else(|| values.get("run").map(|value| vec![value.clone()]))
        .ok_or_else(|| CompileError::MissingSteps(id.clone()))?;
    if run.iter().all(|command| command.trim().is_empty()) {
        return Err(CompileError::MissingSteps(id));
    }
    for (idx, command) in run.iter().enumerate() {
        job.steps.push(Step::run(
            format!("{id}_run_{idx}"),
            format!("run {idx}"),
            command,
        ));
    }
    if let Some(paths) = arrays.get("artifact_paths") {
        job.artifact_paths.push(ArtifactPath {
            name: format!("{id}-artifacts"),
            paths: paths.clone(),
            when: ArtifactWhen::Always,
            retention_days: 14,
        });
    }
    if let Some(paths) = arrays.get("cache_mounts") {
        job.cache_mounts = paths
            .iter()
            .map(|path| CacheMount {
                name: sanitize_id(path),
                path: path.clone(),
                mode: CacheMode::ReadOnly,
                fingerprint: deterministic_hash(path),
            })
            .collect();
    } else {
        job.cache_mounts = default_cache_mounts(&id);
    }
    if let Some(timeout) = values.get("timeout_seconds") {
        job.timeout_seconds = timeout.parse::<u64>().unwrap_or(3600);
    }
    let needs = arrays.get("needs").cloned().unwrap_or_default();
    Ok(PendingJob {
        origin: id,
        job,
        needs: needs.clone(),
    })
}

fn finish_pipeline(
    source: PipelineSource,
    pending: Vec<PendingJob>,
    context: &CompileContext,
) -> Result<Pipeline, CompileError> {
    if pending.is_empty() {
        return Err(CompileError::MissingJobs);
    }
    let mut pipeline = Pipeline::new(
        source,
        &context.repo,
        &context.commit,
        context.trust_tier.clone(),
    );
    let mut origin_to_jobs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for item in &pending {
        origin_to_jobs
            .entry(item.origin.clone())
            .or_default()
            .push(item.job.id.clone());
    }
    for item in &pending {
        pipeline.jobs.push(item.job.clone());
    }
    let mut edge_set = BTreeSet::new();
    for item in &pending {
        for need in &item.needs {
            let from_jobs = origin_to_jobs.get(need).ok_or_else(|| {
                CompileError::InvalidDependency(format!("{} needs unknown job {need}", item.origin))
            })?;
            for from in from_jobs {
                let edge = (from.clone(), item.job.id.clone());
                if edge_set.insert(edge.clone()) {
                    pipeline.edges.push(Dependency {
                        from: edge.0,
                        to: edge.1,
                    });
                }
            }
        }
    }
    pipeline.jobs.sort_by(|a, b| a.id.cmp(&b.id));
    pipeline
        .edges
        .sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));
    Ok(pipeline)
}

fn collect_lines(input: &str) -> Vec<SourceLine> {
    input
        .lines()
        .filter_map(|raw| {
            let without_comment = strip_comment(raw);
            if without_comment.trim().is_empty() {
                return None;
            }
            let indent = without_comment.chars().take_while(|ch| *ch == ' ').count();
            Some(SourceLine {
                indent,
                text: without_comment.trim().to_string(),
            })
        })
        .collect()
}

fn strip_comment(raw: &str) -> &str {
    let trimmed = raw.trim_start();
    if trimmed.starts_with('#') {
        ""
    } else {
        raw
    }
}

fn find_line(lines: &[SourceLine], indent: usize, text: &str) -> Option<usize> {
    lines
        .iter()
        .position(|line| line.indent == indent && line.text == text)
}

fn is_yaml_map_header(text: &str) -> bool {
    text.ends_with(':') && !text.starts_with('-') && !text.contains("::")
}

fn header_name(text: &str) -> String {
    sanitize_id(text.trim_end_matches(':').trim())
}

fn scalar_after<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.strip_prefix(prefix)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn nested_slice(
    lines: &[SourceLine],
    start: usize,
    parent_indent: usize,
) -> (&[SourceLine], usize) {
    let mut end = start;
    while end < lines.len() && lines[end].indent > parent_indent {
        end += 1;
    }
    (&lines[start..end], end)
}

fn parse_steps(origin: &str, lines: &[SourceLine]) -> Result<Vec<Step>, CompileError> {
    let mut builders = Vec::new();
    let mut current: Option<StepBuilder> = None;
    let mut in_env = false;
    let mut env_indent = 0;

    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        if line.text.starts_with("- ") {
            if let Some(builder) = current.take() {
                builders.push(builder);
            }
            current = Some(StepBuilder::default());
            in_env = false;
            let rest = line.text.trim_start_matches("- ").trim();
            if let Some((key, value)) = rest.split_once(':') {
                let value = value.trim();
                if key.trim() == "run" && is_block_scalar(value) {
                    let (body, next) = collect_block_scalar(lines, i + 1, line.indent, value);
                    if let Some(builder) = current.as_mut() {
                        builder.run = Some(body);
                    }
                    i = next;
                    continue;
                }
                apply_step_attr(current.as_mut(), key.trim(), value);
            }
            i += 1;
            continue;
        }
        if current.is_none() {
            i += 1;
            continue;
        }
        if in_env && line.indent > env_indent {
            if let Some((key, value)) = line.text.split_once(':') {
                if let Some(builder) = current.as_mut() {
                    builder.env.insert(
                        key.trim().to_string(),
                        trim_quotes(value.trim()).to_string(),
                    );
                }
            }
            i += 1;
            continue;
        }
        if line.text == "env:" {
            in_env = true;
            env_indent = line.indent;
            i += 1;
            continue;
        }
        in_env = false;
        if let Some((key, value)) = line.text.split_once(':') {
            let value = value.trim();
            if key.trim() == "run" && is_block_scalar(value) {
                let (body, next) = collect_block_scalar(lines, i + 1, line.indent, value);
                if let Some(builder) = current.as_mut() {
                    builder.run = Some(body);
                }
                i = next;
                continue;
            }
            apply_step_attr(current.as_mut(), key.trim(), value);
        }
        i += 1;
    }
    if let Some(builder) = current.take() {
        builders.push(builder);
    }

    builders
        .into_iter()
        .enumerate()
        .map(|(idx, builder)| step_from_builder(origin, idx, builder))
        .collect()
}

fn apply_step_attr(builder: Option<&mut StepBuilder>, key: &str, value: &str) {
    if let Some(builder) = builder {
        match key {
            "name" => builder.name = Some(trim_quotes(value).to_string()),
            "run" => builder.run = Some(trim_quotes(value).to_string()),
            "uses" => builder.uses = Some(trim_quotes(value).to_string()),
            "working-directory" => builder.working_directory = Some(trim_quotes(value).to_string()),
            _ => {}
        }
    }
}

fn step_from_builder(origin: &str, idx: usize, builder: StepBuilder) -> Result<Step, CompileError> {
    if builder.run.as_ref().is_none_or(|run| run.trim().is_empty())
        && builder
            .uses
            .as_ref()
            .is_none_or(|uses| uses.trim().is_empty())
    {
        return Err(CompileError::MissingSteps(origin.to_string()));
    }
    let id = format!("step_{idx:02}");
    let name = builder.name.clone().unwrap_or_else(|| {
        builder
            .run
            .clone()
            .or(builder.uses.clone())
            .unwrap_or_else(|| "step".to_string())
    });
    Ok(Step {
        id,
        name,
        command: builder.run,
        uses: builder.uses,
        env: builder.env,
        working_directory: builder.working_directory,
    })
}

fn steps_with_matrix(steps: &[Step], matrix: &BTreeMap<String, String>, job_id: &str) -> Vec<Step> {
    steps
        .iter()
        .enumerate()
        .map(|(idx, step)| {
            let mut next = step.clone();
            next.id = format!("{job_id}_step_{idx:02}");
            for (key, value) in matrix {
                let needle = format!("${{{{ matrix.{key} }}}}");
                if let Some(command) = next.command.as_mut() {
                    *command = command.replace(&needle, value);
                }
                if let Some(uses) = next.uses.as_mut() {
                    *uses = uses.replace(&needle, value);
                }
                next.name = next.name.replace(&needle, value);
            }
            next
        })
        .collect()
}

fn parse_matrix(lines: &[SourceLine]) -> BTreeMap<String, Vec<String>> {
    let mut matrix = BTreeMap::new();
    let mut inside_matrix = false;
    let mut matrix_indent = 0;
    for line in lines {
        if line.text == "matrix:" {
            inside_matrix = true;
            matrix_indent = line.indent;
            continue;
        }
        if inside_matrix && line.indent > matrix_indent {
            if let Some((key, value)) = line.text.split_once(':') {
                matrix.insert(key.trim().to_string(), parse_array_or_scalar(value.trim()));
            }
        }
    }
    matrix
}

fn matrix_combinations(matrix: &BTreeMap<String, Vec<String>>) -> Vec<BTreeMap<String, String>> {
    if matrix.is_empty() {
        return vec![BTreeMap::new()];
    }
    let mut combos = vec![BTreeMap::new()];
    for (key, values) in matrix {
        let mut next = Vec::new();
        for combo in &combos {
            for value in values {
                let mut expanded = combo.clone();
                expanded.insert(key.clone(), value.clone());
                next.push(expanded);
            }
        }
        combos = next;
    }
    combos
}

fn expanded_id(origin: &str, combo: &BTreeMap<String, String>) -> String {
    if combo.is_empty() {
        return sanitize_id(origin);
    }
    let suffix = combo
        .iter()
        .map(|(k, v)| format!("{k}_{v}"))
        .collect::<Vec<_>>()
        .join("__");
    sanitize_id(&format!("{origin}__{suffix}"))
}

fn expanded_name(origin: &str, combo: &BTreeMap<String, String>) -> String {
    if combo.is_empty() {
        origin.to_string()
    } else {
        let suffix = combo
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{origin} ({suffix})")
    }
}

fn parse_array_or_scalar(value: &str) -> Vec<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Vec::new();
    }
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len().saturating_sub(1)];
        return inner
            .split(',')
            .map(|item| trim_quotes(item.trim()).to_string())
            .filter(|item| !item.is_empty())
            .collect();
    }
    vec![trim_quotes(trimmed).to_string()]
}

fn parse_yaml_list(lines: &[SourceLine]) -> Vec<String> {
    lines
        .iter()
        .filter_map(|line| line.text.strip_prefix("- "))
        .map(|item| trim_quotes(item.trim()).to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn is_block_scalar(value: &str) -> bool {
    matches!(value.trim(), "|" | "|-" | "|+" | ">" | ">-" | ">+")
}

fn collect_block_scalar(
    lines: &[SourceLine],
    start: usize,
    parent_indent: usize,
    marker: &str,
) -> (String, usize) {
    let mut body = Vec::new();
    let mut end = start;
    while end < lines.len() && lines[end].indent > parent_indent {
        body.push(lines[end].text.clone());
        end += 1;
    }
    if marker.trim().starts_with('>') {
        (body.join(" "), end)
    } else {
        (body.join("\n"), end)
    }
}

fn parse_cache_mounts(lines: &[SourceLine], origin: &str) -> Vec<CacheMount> {
    let mut paths = Vec::new();
    let mut in_paths = false;
    let mut path_indent = 0;
    for line in lines {
        if let Some(value) = scalar_after(&line.text, "paths:") {
            paths.extend(parse_array_or_scalar(value));
            in_paths = true;
            path_indent = line.indent;
            continue;
        }
        if line.text == "paths:" {
            in_paths = true;
            path_indent = line.indent;
            continue;
        }
        if in_paths && line.indent > path_indent {
            if let Some(value) = line.text.strip_prefix("- ") {
                paths.push(trim_quotes(value.trim()).to_string());
            }
        }
    }
    paths
        .into_iter()
        .map(|path| CacheMount {
            name: format!("{}-{}", sanitize_id(origin), sanitize_id(&path)),
            fingerprint: deterministic_hash(&format!("cache|{origin}|{path}")),
            path,
            mode: CacheMode::ReadOnly,
        })
        .collect()
}

fn default_cache_mounts(job_id: &str) -> Vec<CacheMount> {
    vec![CacheMount {
        name: format!("{job_id}-cargo-target"),
        path: "target/".to_string(),
        mode: CacheMode::ReadOnly,
        fingerprint: deterministic_hash(&format!("default-cache|{job_id}|target")),
    }]
}

fn parse_artifacts(lines: &[SourceLine], origin: &str) -> Vec<ArtifactPath> {
    let mut paths = Vec::new();
    let mut retention_days = 14;
    let mut when = ArtifactWhen::Always;
    let mut in_paths = false;
    let mut path_indent = 0;
    for line in lines {
        if let Some(value) = scalar_after(&line.text, "paths:") {
            paths.extend(parse_array_or_scalar(value));
            in_paths = true;
            path_indent = line.indent;
            continue;
        }
        if line.text == "paths:" {
            in_paths = true;
            path_indent = line.indent;
            continue;
        }
        if let Some(value) = scalar_after(&line.text, "expire_in:") {
            retention_days = parse_retention_days(value);
        }
        if let Some(value) = scalar_after(&line.text, "when:") {
            when = match trim_quotes(value) {
                "on_failure" | "on-failure" => ArtifactWhen::OnFailure,
                "on_success" | "on-success" => ArtifactWhen::OnSuccess,
                _ => ArtifactWhen::Always,
            };
        }
        if in_paths && line.indent > path_indent {
            if let Some(value) = line.text.strip_prefix("- ") {
                paths.push(trim_quotes(value.trim()).to_string());
            }
        }
    }
    if paths.is_empty() {
        Vec::new()
    } else {
        vec![ArtifactPath {
            name: format!("{}-artifacts", sanitize_id(origin)),
            paths,
            when,
            retention_days,
        }]
    }
}

fn parse_token_scope(value: &str) -> TokenScope {
    match trim_quotes(value) {
        "{}" | "none" => TokenScope::None,
        "read" | "read-all" => TokenScope::ReadRepo,
        "write" | "write-all" => TokenScope::WriteChecks,
        other => TokenScope::Custom(parse_array_or_scalar(other)),
    }
}

fn parse_retention_days(value: &str) -> u32 {
    let lower = trim_quotes(value).to_ascii_lowercase();
    let digits: String = lower.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    let n = digits.parse::<u32>().unwrap_or(14);
    if lower.contains("week") {
        n.saturating_mul(7)
    } else if lower.contains("month") {
        n.saturating_mul(30)
    } else {
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> CompileContext {
        CompileContext::new("acme/demo", "abc123")
    }

    #[test]
    fn compiles_github_matrix_and_hash_is_deterministic() -> Result<(), Box<dyn std::error::Error>>
    {
        let input = include_str!("../../../tests/fixtures/github/matrix.yml");
        let a = Compiler::compile(input, CiKind::GitHubActions, ctx())?;
        let b = Compiler::compile(input, CiKind::GitHubActions, ctx())?;
        assert_eq!(a.ir_hash(), b.ir_hash());
        assert_eq!(a.jobs.len(), 5);
        assert!(a
            .jobs
            .iter()
            .any(|job| job.id == "test__os_ubuntu-latest__toolchain_stable"));
        assert!(a
            .edges
            .iter()
            .any(|edge| edge.from == "fmt" && edge.to.contains("test__")));
        Ok(())
    }

    #[test]
    fn compiles_native_toml() -> Result<(), Box<dyn std::error::Error>> {
        let input = include_str!("../../../tests/fixtures/native/ci.toml");
        let pipeline = Compiler::compile(input, CiKind::NativeToml, ctx())?;
        assert_eq!(pipeline.jobs.len(), 3);
        assert!(pipeline
            .edges
            .iter()
            .any(|edge| edge.from == "check" && edge.to == "test"));
        Ok(())
    }

    #[test]
    fn parses_block_form_needs() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"
name: block needs
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - run: cargo check
  test:
    runs-on: ubuntu-latest
    needs:
      - build
    steps:
      - run: cargo test
"#;
        let pipeline = Compiler::compile(input, CiKind::GitHubActions, ctx())?;
        assert!(pipeline
            .edges
            .iter()
            .any(|edge| edge.from == "build" && edge.to == "test"));
        Ok(())
    }

    #[test]
    fn parses_multiline_run_body() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"
name: multiline
jobs:
  script:
    runs-on: ubuntu-latest
    steps:
      - name: shell body
        run: |
          echo prepare
          cargo test --workspace
"#;
        let pipeline = Compiler::compile(input, CiKind::GitHubActions, ctx())?;
        let command = pipeline.jobs[0].steps[0]
            .command
            .as_deref()
            .expect("run command");
        assert!(command.contains("echo prepare"));
        assert!(command.contains("cargo test --workspace"));
        assert_ne!(command, "|");
        Ok(())
    }

    #[test]
    fn github_job_without_executable_step_fails_closed() {
        let input = r#"
name: missing step
jobs:
  inspect:
    runs-on: ubuntu-latest
    steps:
      - name: metadata only
"#;
        let err = Compiler::compile(input, CiKind::GitHubActions, ctx())
            .expect_err("metadata-only step must fail closed");
        assert_eq!(err, CompileError::MissingSteps("inspect".to_string()));
    }

    #[test]
    fn parses_500_job_fixture() -> Result<(), Box<dyn std::error::Error>> {
        let input = include_str!("../../../tests/fixtures/github/500_jobs.yml");
        let pipeline = Compiler::compile(input, CiKind::GitHubActions, ctx())?;
        assert_eq!(pipeline.jobs.len(), 500);
        assert_eq!(pipeline.edges.len(), 499);
        let hash = pipeline.ir_hash();
        assert!(hash.starts_with("fnv64:"));
        Ok(())
    }
}
