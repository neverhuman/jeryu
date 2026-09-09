//! Parsers for job-level attributes: cache mounts, artifacts, token scopes, and
//! retention windows, plus the default cache mount applied when none is given.

use jeryu_ci_ir::{
    ArtifactPath, ArtifactWhen, CacheMode, CacheMount, TokenScope, deterministic_hash, sanitize_id,
    trim_quotes,
};

use crate::lexer::{SourceLine, parse_array_or_scalar, scalar_after};

pub(crate) fn parse_cache_mounts(lines: &[SourceLine], origin: &str) -> Vec<CacheMount> {
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
        if in_paths
            && line.indent > path_indent
            && let Some(value) = line.text.strip_prefix("- ")
        {
            paths.push(trim_quotes(value.trim()).to_string());
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

pub(crate) fn default_cache_mounts(job_id: &str) -> Vec<CacheMount> {
    vec![CacheMount {
        name: format!("{job_id}-cargo-target"),
        path: "target/".to_string(),
        mode: CacheMode::ReadOnly,
        fingerprint: deterministic_hash(&format!("default-cache|{job_id}|target")),
    }]
}

pub(crate) fn parse_artifacts(lines: &[SourceLine], origin: &str) -> Vec<ArtifactPath> {
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
        if in_paths
            && line.indent > path_indent
            && let Some(value) = line.text.strip_prefix("- ")
        {
            paths.push(trim_quotes(value.trim()).to_string());
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

pub(crate) fn parse_token_scope(value: &str) -> TokenScope {
    match trim_quotes(value) {
        "{}" | "none" => TokenScope::None,
        "read" | "read-all" => TokenScope::ReadRepo,
        "write" | "write-all" => TokenScope::WriteChecks,
        other => TokenScope::Custom(parse_array_or_scalar(other)),
    }
}

pub(crate) fn parse_retention_days(value: &str) -> u32 {
    let lower = trim_quotes(value).to_ascii_lowercase();
    let digits: String = lower.chars().take_while(|ch| ch.is_ascii_digit()).collect();
    // `digits` is empty only when no leading number was supplied; 14 days is the
    // documented default retention, so an absent count is the intended value
    // rather than a swallowed parse error.
    let n = digits.parse::<u32>().unwrap_or(14);
    if lower.contains("week") {
        n.saturating_mul(7)
    } else if lower.contains("month") {
        n.saturating_mul(30)
    } else {
        n
    }
}
