//! Fast repeated-code clustering for Jankurai tool-building leads.
//!
//! The v1 scanner is intentionally deterministic and cheap: it walks source
//! files, builds identifier/literal-normalized line windows, hashes them, and
//! ranks repeated windows into tool-building opportunity clusters. Deeper AST,
//! LSH, and API-motif analyzers can attach to the same persisted cluster shape.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{CodeGraphError, Result};

/// Runtime knobs for the fast tool-building scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildScanConfig {
    /// Number of normalized non-empty lines in each fingerprinted window.
    pub window_lines: usize,
    /// Minimum normalized tokens in a window before it can form a cluster.
    pub min_normalized_tokens: usize,
    /// Minimum occurrences required for a cluster.
    pub min_occurrences: usize,
    /// Max bytes to read per source file; larger files are skipped.
    pub max_file_bytes: u64,
    /// Max ranked clusters returned to callers.
    pub max_clusters: usize,
}

impl Default for ToolBuildScanConfig {
    fn default() -> Self {
        Self {
            window_lines: 8,
            min_normalized_tokens: 36,
            min_occurrences: 2,
            max_file_bytes: 512 * 1024,
            max_clusters: 50,
        }
    }
}

/// Result of a tool-building scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildScanReport {
    /// Stable repository id for the scanned root.
    pub repo_id: String,
    /// Commit or ref the scan represents.
    pub commit_sha: String,
    /// Root that was scanned.
    pub root: String,
    /// Scan timestamp in unix milliseconds.
    pub scanned_at: String,
    /// Files considered by the scanner.
    pub scanned_files: usize,
    /// Files skipped by size, binary decode, or excluded path.
    pub skipped_files: usize,
    /// Ranked repeated-code clusters.
    pub clusters: Vec<ToolBuildCluster>,
}

/// Ranked repeated-code cluster that can become a Jankurai tool-building task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildCluster {
    /// Stable cluster id derived from the normalized fingerprint.
    pub cluster_id: String,
    /// Stable repository id for the scan that produced the cluster.
    pub repo_id: String,
    /// Commit or ref the scan represents.
    pub commit_sha: String,
    /// BLAKE3 fingerprint of the normalized window.
    pub fingerprint: String,
    /// Heuristic score for ranking possible tool-building opportunities.
    pub score: u64,
    /// Number of occurrences in the cluster.
    pub occurrence_count: usize,
    /// Number of distinct repos represented by the cluster.
    pub repo_count: usize,
    /// Number of distinct files represented by the cluster.
    pub file_count: usize,
    /// Total duplicated source lines covered by all occurrences.
    pub total_lines: usize,
    /// Dominant language label.
    pub language: String,
    /// Short deterministic insight for polling clients.
    pub insight: String,
    /// Normalized preview used for explanation and future AI review.
    pub normalized_preview: String,
    /// Representative occurrences, capped for compact MCP responses.
    pub occurrences: Vec<ToolBuildOccurrence>,
    /// Optional ignore feedback from the durable store.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignored: Option<ToolBuildIgnore>,
}

/// One code occurrence in a repeated-code cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildOccurrence {
    /// Stable repository id.
    pub repo_id: String,
    /// Commit or ref the scan represents.
    pub commit_sha: String,
    /// Repo-relative path.
    pub path: String,
    /// 1-based start line.
    pub start_line: usize,
    /// 1-based end line.
    pub end_line: usize,
    /// Language/domain label.
    pub language: String,
    /// Normalized-token count for the occurrence window.
    pub normalized_token_count: usize,
}

/// Durable ignore feedback for a cluster.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolBuildIgnore {
    /// Ignored cluster id.
    pub cluster_id: String,
    /// Human/agent reason the cluster should not produce tool-building work.
    pub reason: String,
    /// Actor that recorded the feedback.
    pub ignored_by: String,
    /// Feedback timestamp in unix milliseconds.
    pub ignored_at: String,
}

#[derive(Debug, Clone)]
struct NormalizedLine {
    line_number: usize,
    tokens: Vec<String>,
}

#[derive(Debug, Clone)]
struct ClusterBuilder {
    fingerprint: String,
    normalized_preview: String,
    occurrence_count: usize,
    total_lines: usize,
    token_total: usize,
    files: BTreeSet<String>,
    languages: BTreeMap<String, usize>,
    occurrences: Vec<ToolBuildOccurrence>,
}

impl ClusterBuilder {
    fn push(&mut self, occurrence: ToolBuildOccurrence, window_lines: usize) {
        self.occurrence_count += 1;
        self.total_lines += window_lines;
        self.token_total += occurrence.normalized_token_count;
        self.files.insert(occurrence.path.clone());
        *self
            .languages
            .entry(occurrence.language.clone())
            .or_default() += 1;
        if self.occurrences.len() < 12 {
            self.occurrences.push(occurrence);
        }
    }

    fn into_cluster(self, repo_id: &str, commit_sha: &str) -> ToolBuildCluster {
        let language = self
            .languages
            .iter()
            .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
            .map(|(language, _)| language.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let file_count = self.files.len();
        let score = (self.occurrence_count as u64)
            .saturating_mul(self.token_total as u64)
            .saturating_add((file_count as u64).saturating_mul(100))
            .saturating_add(self.total_lines as u64);
        let insight = cluster_insight(
            self.occurrence_count,
            file_count,
            self.total_lines,
            &language,
            &self.normalized_preview,
        );
        ToolBuildCluster {
            cluster_id: format!("toolbuild-{}", &self.fingerprint[..16]),
            repo_id: repo_id.to_string(),
            commit_sha: commit_sha.to_string(),
            fingerprint: self.fingerprint,
            score,
            occurrence_count: self.occurrence_count,
            repo_count: 1,
            file_count,
            total_lines: self.total_lines,
            language,
            insight,
            normalized_preview: self.normalized_preview,
            occurrences: self.occurrences,
            ignored: None,
        }
    }
}

/// Scan a repository root and return ranked repeated-code clusters.
pub fn scan_tool_build_clusters(
    root: impl AsRef<Path>,
    repo_id: impl Into<String>,
    commit_sha: impl Into<String>,
    config: ToolBuildScanConfig,
) -> Result<ToolBuildScanReport> {
    let root = root.as_ref();
    let repo_id = repo_id.into();
    let commit_sha = commit_sha.into();
    let mut files = Vec::new();
    collect_source_files(root, root, &mut files)?;
    files.sort();

    let mut scanned_files = 0;
    let mut skipped_files = 0;
    let mut clusters: BTreeMap<String, ClusterBuilder> = BTreeMap::new();
    let window_lines = config.window_lines.max(2);

    for path in files {
        let metadata = std::fs::metadata(&path).map_err(|source| CodeGraphError::Index {
            path: path.display().to_string(),
            source,
        })?;
        if metadata.len() > config.max_file_bytes {
            skipped_files += 1;
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else {
            skipped_files += 1;
            continue;
        };
        scanned_files += 1;
        let relative = repo_relative(root, &path);
        let language = language_for_path(&path);
        let normalized = normalized_lines(&contents);
        if normalized.len() < window_lines {
            continue;
        }
        for window in normalized.windows(window_lines) {
            let normalized_window = window
                .iter()
                .map(|line| line.tokens.join(" "))
                .collect::<Vec<_>>()
                .join("\n");
            let token_count = window.iter().map(|line| line.tokens.len()).sum();
            if token_count < config.min_normalized_tokens {
                continue;
            }
            let fingerprint = blake3::hash(normalized_window.as_bytes())
                .to_hex()
                .to_string();
            let start_line = window.first().map(|line| line.line_number).unwrap_or(1);
            let end_line = window
                .last()
                .map(|line| line.line_number)
                .unwrap_or(start_line);
            let occurrence = ToolBuildOccurrence {
                repo_id: repo_id.clone(),
                commit_sha: commit_sha.clone(),
                path: relative.clone(),
                start_line,
                end_line,
                language: language.clone(),
                normalized_token_count: token_count,
            };
            clusters
                .entry(fingerprint.clone())
                .or_insert_with(|| ClusterBuilder {
                    fingerprint,
                    normalized_preview: normalized_window,
                    occurrence_count: 0,
                    total_lines: 0,
                    token_total: 0,
                    files: BTreeSet::new(),
                    languages: BTreeMap::new(),
                    occurrences: Vec::new(),
                })
                .push(occurrence, window_lines);
        }
    }

    let mut clusters: Vec<_> = clusters
        .into_values()
        .filter(|cluster| cluster.occurrence_count >= config.min_occurrences.max(2))
        .map(|cluster| cluster.into_cluster(&repo_id, &commit_sha))
        .collect();
    clusters.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.occurrence_count.cmp(&a.occurrence_count))
            .then_with(|| a.cluster_id.cmp(&b.cluster_id))
    });
    clusters.truncate(config.max_clusters.max(1));

    Ok(ToolBuildScanReport {
        repo_id,
        commit_sha,
        root: root.display().to_string(),
        scanned_at: epoch_millis(),
        scanned_files,
        skipped_files,
        clusters,
    })
}

fn collect_source_files(root: &Path, current: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = std::fs::read_dir(current).map_err(|source| CodeGraphError::Index {
        path: current.display().to_string(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| CodeGraphError::Index {
            path: current.display().to_string(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| CodeGraphError::Index {
            path: path.display().to_string(),
            source,
        })?;
        if file_type.is_dir() {
            if should_skip_dir(root, &path) {
                continue;
            }
            collect_source_files(root, &path, out)?;
        } else if file_type.is_file() && is_source_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn should_skip_dir(root: &Path, path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    matches!(
        name,
        ".git"
            | "target"
            | "node_modules"
            | ".jankurai"
            | "dist"
            | "playwright-report"
            | "storybook-static"
    ) || repo_relative(root, path).starts_with("docs/generated/")
}

fn is_source_file(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    matches!(
        ext,
        "rs" | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "go"
            | "py"
            | "java"
            | "kt"
            | "rb"
            | "sh"
            | "toml"
            | "yaml"
            | "yml"
            | "json"
    )
}

fn language_for_path(path: &Path) -> String {
    match path.extension().and_then(|ext| ext.to_str()).unwrap_or("") {
        "rs" => "rust",
        "ts" => "typescript",
        "tsx" => "typescript_react",
        "js" => "javascript",
        "jsx" => "javascript_react",
        "go" => "go",
        "py" => "python",
        "java" => "java",
        "kt" => "kotlin",
        "rb" => "ruby",
        "sh" => "shell",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        _ => "unknown",
    }
    .to_string()
}

fn repo_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn normalized_lines(contents: &str) -> Vec<NormalizedLine> {
    contents
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let tokens = normalize_line(line);
            (!tokens.is_empty()).then_some(NormalizedLine {
                line_number: idx + 1,
                tokens,
            })
        })
        .collect()
}

fn normalize_line(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if trimmed.is_empty()
        || trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("/*")
    {
        return Vec::new();
    }
    let mut tokens = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' || ch == '\'' {
            tokens.push("lit:str".to_string());
            i += 1;
            while i < chars.len() {
                let current = chars[i];
                let escaped = i > 0 && chars[i - 1] == '\\';
                i += 1;
                if current == ch && !escaped {
                    break;
                }
            }
        } else if ch.is_ascii_digit() {
            tokens.push("lit:num".to_string());
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
        } else if is_ident_start(ch) {
            let start = i;
            i += 1;
            while i < chars.len() && is_ident_continue(chars[i]) {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();
            let lower = ident.to_ascii_lowercase();
            let next = next_non_ws(&chars, i);
            let prev = prev_non_ws(&chars, start);
            if is_keyword(&lower) {
                tokens.push(format!("kw:{lower}"));
            } else if next == Some('!') {
                tokens.push(format!("macro:{lower}"));
            } else if next == Some('(') {
                tokens.push(format!("call:{lower}"));
            } else if matches!(prev, Some('.') | Some(':')) {
                tokens.push(format!("member:{lower}"));
            } else {
                tokens.push("id".to_string());
            }
        } else if is_operator(ch) {
            tokens.push(format!("op:{ch}"));
            i += 1;
        } else {
            i += 1;
        }
    }
    tokens
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

fn is_operator(ch: char) -> bool {
    matches!(
        ch,
        '{' | '}' | '(' | ')' | '[' | ']' | '?' | '=' | '>' | '<' | '&' | '|'
    )
}

fn next_non_ws(chars: &[char], mut idx: usize) -> Option<char> {
    while idx < chars.len() {
        if !chars[idx].is_whitespace() {
            return Some(chars[idx]);
        }
        idx += 1;
    }
    None
}

fn prev_non_ws(chars: &[char], mut idx: usize) -> Option<char> {
    while idx > 0 {
        idx -= 1;
        if !chars[idx].is_whitespace() {
            return Some(chars[idx]);
        }
    }
    None
}

fn is_keyword(value: &str) -> bool {
    matches!(
        value,
        "as" | "async"
            | "await"
            | "break"
            | "class"
            | "const"
            | "continue"
            | "else"
            | "enum"
            | "false"
            | "fn"
            | "for"
            | "function"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "mut"
            | "pub"
            | "return"
            | "self"
            | "static"
            | "struct"
            | "trait"
            | "true"
            | "type"
            | "use"
            | "where"
            | "while"
    )
}

fn cluster_insight(
    occurrence_count: usize,
    file_count: usize,
    total_lines: usize,
    language: &str,
    preview: &str,
) -> String {
    let anchors = preview
        .split_whitespace()
        .filter(|token| {
            token.starts_with("call:")
                || token.starts_with("macro:")
                || matches!(
                    *token,
                    "kw:if" | "kw:match" | "kw:for" | "kw:while" | "kw:return"
                )
        })
        .take(8)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ");
    if anchors.is_empty() {
        format!(
            "{language} normalized window repeats {occurrence_count} times across {file_count} file(s), covering {total_lines} lines; inspect as a possible small helper or codemod."
        )
    } else {
        format!(
            "{language} normalized window repeats {occurrence_count} times across {file_count} file(s), covering {total_lines} lines; anchors: {anchors}."
        )
    }
}

fn epoch_millis() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| "0".to_string())
}
