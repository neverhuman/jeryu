//! Second-tier pattern families: clusters whose anchor signatures overlap are
//! variants of one repeated pattern even when their exact normalized windows
//! differ. Families are a pure function of the cluster list, so they can be
//! recomputed from persisted rows at read time — no second source of truth.

use std::collections::{BTreeMap, BTreeSet};

use super::{ToolBuildCluster, ToolBuildClusterFamily, enrich};

/// Jaccard similarity floor (x100) for grouping two anchor signatures.
const JACCARD_FLOOR_X100: usize = 60;
/// Minimum anchor-signature size before a cluster can group with another.
const MIN_SIGNATURE: usize = 2;

/// Group ranked clusters into pattern families. Grouping never crosses a
/// (language, category) boundary, ordering is deterministic, and singleton
/// clusters still emit a family of one so dashboards have a uniform model.
#[must_use]
pub fn group_pattern_families(clusters: &[ToolBuildCluster]) -> Vec<ToolBuildClusterFamily> {
    // Bucket cluster indexes by (language, category).
    let mut buckets: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (idx, cluster) in clusters.iter().enumerate() {
        buckets
            .entry((
                cluster.language.clone(),
                cluster.category.as_str().to_string(),
            ))
            .or_default()
            .push(idx);
    }

    let signatures: Vec<BTreeSet<String>> = clusters.iter().map(anchor_signature).collect();

    // Union-find with path halving; roots re-canonicalized afterwards to the
    // member with the lexically smallest cluster_id for determinism.
    let mut parent: Vec<usize> = (0..clusters.len()).collect();
    fn find(parent: &mut [usize], mut node: usize) -> usize {
        while parent[node] != node {
            parent[node] = parent[parent[node]];
            node = parent[node];
        }
        node
    }

    for indexes in buckets.values() {
        for (a_pos, &a) in indexes.iter().enumerate() {
            if signatures[a].len() < MIN_SIGNATURE {
                continue;
            }
            for &b in &indexes[a_pos + 1..] {
                if signatures[b].len() < MIN_SIGNATURE {
                    continue;
                }
                let intersection = signatures[a].intersection(&signatures[b]).count();
                let union = signatures[a].len() + signatures[b].len() - intersection;
                if union == 0 || intersection * 100 < JACCARD_FLOOR_X100 * union {
                    continue;
                }
                let root_a = find(&mut parent, a);
                let root_b = find(&mut parent, b);
                if root_a != root_b {
                    parent[root_a.max(root_b)] = root_a.min(root_b);
                }
            }
        }
    }

    // Collect members per root.
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for idx in 0..clusters.len() {
        let root = find(&mut parent, idx);
        groups.entry(root).or_default().push(idx);
    }

    let mut families: Vec<ToolBuildClusterFamily> = Vec::with_capacity(groups.len());
    for members in groups.values() {
        families.push(build_family(clusters, members, &signatures));
    }
    families.sort_by(|a, b| {
        b.anticipated_loc_saved_total
            .cmp(&a.anticipated_loc_saved_total)
            .then_with(|| b.score_total.cmp(&a.score_total))
            .then_with(|| a.family_id.cmp(&b.family_id))
    });
    families
}

/// The dedup'd `call:`/`macro:`/`member:` tokens of a cluster's preview.
fn anchor_signature(cluster: &ToolBuildCluster) -> BTreeSet<String> {
    cluster
        .normalized_preview
        .split_whitespace()
        .filter(|token| {
            token.starts_with("call:")
                || token.starts_with("macro:")
                || token.starts_with("member:")
        })
        .map(str::to_string)
        .collect()
}

fn build_family(
    clusters: &[ToolBuildCluster],
    members: &[usize],
    signatures: &[BTreeSet<String>],
) -> ToolBuildClusterFamily {
    let mut cluster_ids: Vec<String> = Vec::with_capacity(members.len());
    let mut repo_ids: BTreeSet<String> = BTreeSet::new();
    let mut union_signature: BTreeSet<String> = BTreeSet::new();
    let mut anchor_frequency: BTreeMap<String, usize> = BTreeMap::new();
    let mut occurrence_total = 0;
    let mut file_total = 0;
    let mut anticipated_total = 0;
    let mut score_total: u64 = 0;
    let mut language = String::new();
    let mut category = super::ToolBuildCategory::ToolCandidate;

    for &idx in members {
        let cluster = &clusters[idx];
        cluster_ids.push(cluster.cluster_id.clone());
        for occ in &cluster.occurrences {
            repo_ids.insert(occ.repo_id.clone());
        }
        for anchor in &signatures[idx] {
            union_signature.insert(anchor.clone());
            *anchor_frequency.entry(anchor.clone()).or_default() += 1;
        }
        occurrence_total += cluster.occurrence_count;
        file_total += cluster.file_count;
        anticipated_total += enrich::anticipated_loc_saved(cluster);
        score_total = score_total.saturating_add(cluster.score);
        language = cluster.language.clone();
        category = cluster.category;
    }
    cluster_ids.sort();

    // Label: the three most frequent anchors, ties broken lexically, with the
    // role prefix stripped for humans.
    let mut ranked: Vec<(&String, &usize)> = anchor_frequency.iter().collect();
    ranked.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    let label_anchors: Vec<&str> = ranked
        .iter()
        .take(3)
        .map(|(anchor, _)| {
            anchor
                .split_once(':')
                .map_or(anchor.as_str(), |(_, name)| name)
        })
        .collect();
    let label = if label_anchors.is_empty() {
        format!("{language} pattern")
    } else {
        label_anchors.join(", ")
    };

    let fingerprint = blake3::hash(
        format!(
            "{language}:{}:{}",
            category.as_str(),
            union_signature
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string();

    ToolBuildClusterFamily {
        family_id: format!("toolfam-{}", &fingerprint[..16]),
        label,
        language,
        category,
        cluster_count: cluster_ids.len(),
        cluster_ids,
        repo_ids: repo_ids.into_iter().collect(),
        occurrence_total,
        file_total,
        anticipated_loc_saved_total: anticipated_total,
        score_total,
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ToolBuildCategory, ToolBuildCluster, ToolBuildOccurrence};
    use super::*;

    fn cluster(id: &str, language: &str, preview: &str, repos: &[&str]) -> ToolBuildCluster {
        ToolBuildCluster {
            cluster_id: id.to_string(),
            repo_id: "system/host".to_string(),
            commit_sha: "working-tree".to_string(),
            fingerprint: format!("{id}-fp"),
            score: 100,
            occurrence_count: repos.len(),
            repo_count: repos.len(),
            file_count: repos.len(),
            total_lines: 40,
            language: language.to_string(),
            insight: String::new(),
            normalized_preview: preview.to_string(),
            category: ToolBuildCategory::ToolCandidate,
            member_cluster_ids: Vec::new(),
            occurrences: repos
                .iter()
                .map(|repo| ToolBuildOccurrence {
                    repo_id: (*repo).to_string(),
                    commit_sha: "working-tree".to_string(),
                    path: "src/lib.rs".to_string(),
                    start_line: 1,
                    end_line: 10,
                    language: language.to_string(),
                    normalized_token_count: 40,
                    is_test: false,
                })
                .collect(),
            ignored: None,
        }
    }

    #[test]
    fn overlapping_anchor_signatures_group() {
        let a = cluster(
            "toolbuild-aaa",
            "rust",
            "kw:let id op:= call:retry op:( id op:)\nmember:is_ok call:call_remote",
            &["repo-a", "repo-b"],
        );
        let b = cluster(
            "toolbuild-bbb",
            "rust",
            "kw:let id op:= call:retry op:( lit:num op:)\nmember:is_ok call:call_remote kw:return",
            &["repo-b", "repo-c"],
        );
        let families = group_pattern_families(&[a, b]);
        assert_eq!(families.len(), 1);
        let family = &families[0];
        assert_eq!(family.cluster_count, 2);
        assert_eq!(family.repo_ids, vec!["repo-a", "repo-b", "repo-c"]);
        assert!(family.label.contains("retry"));
    }

    #[test]
    fn languages_never_mix_and_output_is_deterministic() {
        let a = cluster(
            "toolbuild-aaa",
            "rust",
            "call:retry member:is_ok",
            &["r1", "r2"],
        );
        let b = cluster(
            "toolbuild-bbb",
            "typescript",
            "call:retry member:is_ok",
            &["r1", "r2"],
        );
        let forward = group_pattern_families(&[a.clone(), b.clone()]);
        let reversed = group_pattern_families(&[b, a]);
        assert_eq!(forward.len(), 2);
        let forward_ids: Vec<_> = forward.iter().map(|f| f.family_id.clone()).collect();
        let reversed_ids: Vec<_> = reversed.iter().map(|f| f.family_id.clone()).collect();
        assert_eq!(forward_ids, reversed_ids);
    }

    #[test]
    fn tiny_signatures_stay_singletons() {
        let a = cluster("toolbuild-aaa", "rust", "call:retry", &["r1", "r2"]);
        let b = cluster("toolbuild-bbb", "rust", "call:retry", &["r2", "r3"]);
        let families = group_pattern_families(&[a, b]);
        assert_eq!(families.len(), 2);
    }
}
