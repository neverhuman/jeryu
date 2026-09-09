//! Dossier enrichment shared by the finder CLI and the API server: how many
//! lines a shared tool would save, what kind of tool to suggest, and a human
//! name. Pure functions of the cluster, so every consumer agrees.

use std::collections::BTreeSet;

use super::ToolBuildCluster;

/// Suggested registry metadata for proposing a cluster as a shared tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClusterEnrichment {
    /// Lines saved if all occurrences collapse into one shared implementation
    /// (total duplicated lines minus one retained copy).
    pub anticipated_loc_saved: usize,
    /// Registry tool kind for the dominant language.
    pub suggested_kind: &'static str,
    /// Human-readable proposal name.
    pub suggested_name: String,
    /// Short anchor label mined from the normalized preview.
    pub anchor_label: String,
}

/// Enrich a cluster with proposal metadata. Mirrors the historical
/// `dossier.py` heuristics so existing dossiers keep their meaning.
#[must_use]
pub fn enrich_cluster(cluster: &ToolBuildCluster) -> ClusterEnrichment {
    let anchor_label = anchor_label(cluster);
    ClusterEnrichment {
        anticipated_loc_saved: anticipated_loc_saved(cluster),
        suggested_kind: suggested_kind(&cluster.language),
        suggested_name: format!("Shared {} helper ({anchor_label})", cluster.language),
        anchor_label,
    }
}

/// Lines saved by deduplicating: every duplicated line except one retained
/// copy (the first occurrence's span).
#[must_use]
pub fn anticipated_loc_saved(cluster: &ToolBuildCluster) -> usize {
    let retained = cluster
        .occurrences
        .first()
        .map(|occ| occ.end_line.saturating_sub(occ.start_line) + 1)
        .unwrap_or_else(|| cluster.total_lines.max(1));
    cluster.total_lines.saturating_sub(retained)
}

/// Dominant scan language -> registry tool kind (see jeryu-tool's
/// docs/tools-registry.md).
#[must_use]
pub fn suggested_kind(language: &str) -> &'static str {
    match language {
        "rust" => "rust-crate",
        "typescript" | "javascript" => "ts-lib",
        "typescript_react" | "javascript_react" => "react-component",
        "shell" => "shell-lib",
        "toml" | "yaml" | "json" => "config-pattern",
        _ => "rust-crate",
    }
}

/// A short human label mined from the preview's call/macro anchors (first
/// three distinct names), falling back to the language.
#[must_use]
pub fn anchor_label(cluster: &ToolBuildCluster) -> String {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut ordered: Vec<&str> = Vec::new();
    for token in cluster.normalized_preview.split_whitespace() {
        if let Some(name) = token
            .strip_prefix("call:")
            .or_else(|| token.strip_prefix("macro:"))
        {
            if seen.insert(name) {
                ordered.push(name);
            }
            if ordered.len() >= 3 {
                break;
            }
        }
    }
    if ordered.is_empty() {
        cluster.language.clone()
    } else {
        ordered.join(", ")
    }
}

/// Deterministic one-line insight for polling clients (v1-identical).
pub(crate) fn cluster_insight(
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

#[cfg(test)]
mod tests {
    use super::super::{ToolBuildCategory, ToolBuildCluster, ToolBuildOccurrence};
    use super::*;

    fn cluster() -> ToolBuildCluster {
        ToolBuildCluster {
            cluster_id: "toolbuild-test".to_string(),
            repo_id: "system/host".to_string(),
            commit_sha: "working-tree".to_string(),
            fingerprint: "fp".to_string(),
            score: 100,
            occurrence_count: 3,
            repo_count: 2,
            file_count: 3,
            total_lines: 36,
            language: "rust".to_string(),
            insight: String::new(),
            normalized_preview:
                "kw:let id op:= call:retry op:( id op:)\nmacro:assert_eq member:unwrap".to_string(),
            category: ToolBuildCategory::ToolCandidate,
            member_cluster_ids: Vec::new(),
            occurrences: vec![ToolBuildOccurrence {
                repo_id: "repo-a".to_string(),
                commit_sha: "working-tree".to_string(),
                path: "src/lib.rs".to_string(),
                start_line: 10,
                end_line: 21,
                language: "rust".to_string(),
                normalized_token_count: 40,
                is_test: false,
            }],
            ignored: None,
        }
    }

    #[test]
    fn enrichment_mirrors_dossier_heuristics() {
        let enriched = enrich_cluster(&cluster());
        // 36 total lines minus the 12-line retained copy.
        assert_eq!(enriched.anticipated_loc_saved, 24);
        assert_eq!(enriched.suggested_kind, "rust-crate");
        assert_eq!(enriched.anchor_label, "retry, assert_eq");
        assert!(enriched.suggested_name.contains("Shared rust helper"));
    }

    #[test]
    fn kind_map_covers_known_languages() {
        assert_eq!(suggested_kind("typescript_react"), "react-component");
        assert_eq!(suggested_kind("shell"), "shell-lib");
        assert_eq!(suggested_kind("yaml"), "config-pattern");
        assert_eq!(suggested_kind("go"), "rust-crate");
    }
}
