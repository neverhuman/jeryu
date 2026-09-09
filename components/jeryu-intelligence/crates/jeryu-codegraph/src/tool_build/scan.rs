//! The shared scan core: parallel per-repo workers fingerprint normalized
//! windows into shards, shards fold into one cross-repo index, survivors are
//! (optionally) overlap-merged, then previews/fingerprints are reconstructed
//! from disk for just the surviving clusters.
//!
//! Memory note: the index stores only compact occurrences (no preview strings)
//! keyed by the first 16 bytes of the window's BLAKE3, so multi-million-window
//! system scans stay in the low hundreds of MB. Previews and full fingerprints
//! are recomputed for the few hundred survivors and verified against the index
//! key, which also guards against files changing mid-scan.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::normalize::{NormalizedLine, normalized_lines};
use super::progress::{ToolBuildScanPhase, ToolBuildScanProgress};
use super::walk::{self, PathClass};
use super::{
    ToolBuildCategory, ToolBuildCluster, ToolBuildOccurrence, ToolBuildScanOptions,
    ToolBuildScanReport, enrich, epoch_millis, families, merge,
};
use crate::error::{CodeGraphError, Result};

/// Compact occurrence stored in the fingerprint index during scanning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScanOccurrence {
    pub repo_idx: u32,
    /// Index into the (shard-local, later rebased global) file table.
    pub file_idx: u32,
    /// Window start index in the file's normalized-line space. Overlap
    /// merging chains windows in THIS space, where +1 means "the next
    /// normalized line", regardless of interleaved blanks/comments.
    pub norm_start: u32,
    /// 1-based raw start line.
    pub start_line: u32,
    /// 1-based raw end line.
    pub end_line: u32,
    /// Normalized token count of the window.
    pub token_count: u32,
}

/// One scanned file in the global file table.
#[derive(Debug, Clone)]
pub(crate) struct FileEntry {
    pub abs: PathBuf,
    pub rel: String,
    pub language: String,
    pub class: PathClass,
}

/// Occurrence list that avoids a heap allocation for the (dominant) case of a
/// fingerprint seen exactly once.
#[derive(Debug, Clone)]
enum OccList {
    One(ScanOccurrence),
    Many(Vec<ScanOccurrence>),
}

impl OccList {
    fn push(&mut self, occ: ScanOccurrence) {
        match self {
            Self::One(first) => *self = Self::Many(vec![*first, occ]),
            Self::Many(list) => list.push(occ),
        }
    }

    fn extend_from(&mut self, other: OccList) {
        match other {
            OccList::One(occ) => self.push(occ),
            OccList::Many(list) => {
                for occ in list {
                    self.push(occ);
                }
            }
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::One(_) => 1,
            Self::Many(list) => list.len(),
        }
    }

    fn into_vec(self) -> Vec<ScanOccurrence> {
        match self {
            Self::One(occ) => vec![occ],
            Self::Many(list) => list,
        }
    }
}

/// One worker's output for one repo.
struct Shard {
    repo_idx: usize,
    files: Vec<FileEntry>,
    index: HashMap<u128, OccList>,
    scanned: usize,
    skipped: usize,
    error: Option<CodeGraphError>,
}

/// A surviving cluster before preview reconstruction. `norm_len` grows past
/// `window_lines` when overlap merging chains windows together.
#[derive(Debug, Clone)]
pub(crate) struct ProtoCluster {
    /// Index key of the (chain-head) window.
    pub key: u128,
    /// Occurrences; sorted by (repo, file, norm_start) on the v2 path.
    pub occs: Vec<ScanOccurrence>,
    /// Normalized-line length of the (merged) window.
    pub norm_len: usize,
    /// Index keys of chained member windows (empty unless merged).
    pub member_keys: Vec<u128>,
}

/// Shared scan entry point behind every public scan function.
pub(crate) fn scan_roots(
    roots: &[(String, PathBuf)],
    label_repo_id: &str,
    commit_sha: &str,
    options: &ToolBuildScanOptions,
    on_progress: &(dyn Fn(ToolBuildScanProgress) + Send + Sync),
) -> Result<ToolBuildScanReport> {
    let window_lines = options.base.window_lines.max(2);
    let repo_total = roots.len();
    let files_scanned = AtomicUsize::new(0);
    let files_skipped = AtomicUsize::new(0);
    let repos_done = AtomicUsize::new(0);
    let next_repo = AtomicUsize::new(0);
    let shards: Mutex<Vec<Shard>> = Mutex::new(Vec::with_capacity(repo_total));

    let progress =
        |phase: ToolBuildScanPhase, repo_index: usize, current_repo: &str, clusters: usize| {
            on_progress(ToolBuildScanProgress {
                phase,
                repo_index,
                repo_total,
                repos_done: repos_done.load(Ordering::Relaxed),
                current_repo: current_repo.to_string(),
                files_scanned: files_scanned.load(Ordering::Relaxed),
                files_skipped: files_skipped.load(Ordering::Relaxed),
                clusters_so_far: clusters,
            });
        };

    progress(ToolBuildScanPhase::Discover, 0, "", 0);

    let worker_count = if options.threads == 0 {
        std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(4)
    } else {
        options.threads
    }
    .min(repo_total.max(1))
    .max(1);

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let repo_idx = next_repo.fetch_add(1, Ordering::Relaxed);
                    if repo_idx >= repo_total {
                        break;
                    }
                    let (repo_id, root) = &roots[repo_idx];
                    progress(ToolBuildScanPhase::Scan, repo_idx, repo_id, 0);
                    let shard = scan_one_repo(
                        repo_idx,
                        root,
                        options,
                        window_lines,
                        &files_scanned,
                        &files_skipped,
                        &|| progress(ToolBuildScanPhase::Scan, repo_idx, repo_id, 0),
                    );
                    repos_done.fetch_add(1, Ordering::Relaxed);
                    progress(ToolBuildScanPhase::Scan, repo_idx, repo_id, 0);
                    shards.lock().expect("shard mutex poisoned").push(shard);
                }
            });
        }
    });

    let mut shards = shards.into_inner().expect("shard mutex poisoned");
    shards.sort_by_key(|shard| shard.repo_idx);
    // v1 strict IO semantics: surface the first error in repo order.
    for shard in &mut shards {
        if let Some(error) = shard.error.take() {
            return Err(error);
        }
    }

    progress(ToolBuildScanPhase::Merge, repo_total, "", 0);

    // Fold shards into one cross-repo index. Shards are visited in repo order
    // and per-shard occurrence vectors preserve scan order, so each key's
    // occurrence list matches what a sequential scan would have produced.
    let mut file_table: Vec<FileEntry> = Vec::new();
    let mut index: HashMap<u128, OccList> = HashMap::new();
    let mut scanned_files = 0;
    let mut skipped_files = 0;
    for shard in shards {
        let offset = file_table.len() as u32;
        file_table.extend(shard.files);
        scanned_files += shard.scanned;
        skipped_files += shard.skipped;
        for (key, mut occs) in shard.index {
            rebase_file_indexes(&mut occs, offset);
            match index.entry(key) {
                std::collections::hash_map::Entry::Occupied(mut entry) => {
                    entry.get_mut().extend_from(occs);
                }
                std::collections::hash_map::Entry::Vacant(entry) => {
                    entry.insert(occs);
                }
            }
        }
    }

    // Survivors: enough occurrences, spanning enough repos.
    let min_occurrences = options.base.min_occurrences.max(2);
    let min_repo_count = options.base.min_repo_count.max(1);
    let mut survivors: Vec<ProtoCluster> = Vec::new();
    for (key, occs) in index {
        if occs.len() < min_occurrences {
            continue;
        }
        let occs = occs.into_vec();
        let distinct_repos = occs
            .iter()
            .map(|occ| occ.repo_idx)
            .collect::<BTreeSet<_>>()
            .len();
        if distinct_repos < min_repo_count {
            continue;
        }
        survivors.push(ProtoCluster {
            key,
            occs,
            norm_len: window_lines,
            member_keys: Vec::new(),
        });
    }
    // Deterministic processing order regardless of hash-map iteration.
    survivors.sort_by_key(|proto| proto.key);

    if options.merge_overlaps {
        for proto in &mut survivors {
            proto
                .occs
                .sort_by_key(|occ| (occ.repo_idx, occ.file_idx, occ.norm_start));
        }
        survivors = merge::merge_overlapping(survivors, window_lines);
    }

    // Reconstruct previews + full fingerprints for survivors only.
    let reconstructed = reconstruct_previews(&survivors, &file_table, window_lines);

    progress(
        ToolBuildScanPhase::Finalize,
        repo_total,
        "",
        survivors.len(),
    );

    let mut clusters: Vec<ToolBuildCluster> = Vec::new();
    for (proto, recon) in survivors.iter().zip(reconstructed) {
        let Some(recon) = recon else {
            // File changed/vanished mid-scan; the window no longer exists.
            continue;
        };
        clusters.push(build_cluster(
            proto,
            recon,
            roots,
            &file_table,
            label_repo_id,
            commit_sha,
            options,
            window_lines,
        ));
    }

    clusters.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.occurrence_count.cmp(&a.occurrence_count))
            .then_with(|| a.cluster_id.cmp(&b.cluster_id))
    });
    clusters.truncate(options.base.max_clusters.max(1));

    let families = if options.compat_v1 {
        Vec::new()
    } else {
        progress(ToolBuildScanPhase::Families, repo_total, "", clusters.len());
        families::group_pattern_families(&clusters)
    };

    progress(ToolBuildScanPhase::Finalize, repo_total, "", clusters.len());

    Ok(ToolBuildScanReport {
        repo_id: label_repo_id.to_string(),
        commit_sha: commit_sha.to_string(),
        root: String::new(), // callers overwrite with their root label
        scanned_at: epoch_millis(),
        scanned_files,
        skipped_files,
        clusters,
        families,
    })
}

fn rebase_file_indexes(occs: &mut OccList, offset: u32) {
    match occs {
        OccList::One(occ) => occ.file_idx += offset,
        OccList::Many(list) => {
            for occ in list {
                occ.file_idx += offset;
            }
        }
    }
}

/// Scan one repo into a shard. Tolerant on the v2 path; v1 IO errors are
/// captured for the coordinator to surface.
fn scan_one_repo(
    repo_idx: usize,
    root: &Path,
    options: &ToolBuildScanOptions,
    window_lines: usize,
    files_scanned: &AtomicUsize,
    files_skipped: &AtomicUsize,
    tick: &dyn Fn(),
) -> Shard {
    let mut shard = Shard {
        repo_idx,
        files: Vec::new(),
        index: HashMap::new(),
        scanned: 0,
        skipped: 0,
        error: None,
    };

    let discovered: Vec<FileEntry> = if options.compat_v1 {
        let mut paths = Vec::new();
        if let Err(error) = walk::collect_source_files_v1(root, root, &mut paths) {
            shard.error = Some(error);
            return shard;
        }
        paths.sort();
        paths
            .into_iter()
            .map(|abs| FileEntry {
                rel: walk::repo_relative(root, &abs),
                language: walk::language_for_path(&abs),
                class: PathClass::Source,
                abs,
            })
            .collect()
    } else {
        let (files, dropped) = walk::collect_repo_files(root, options);
        if dropped > 0 {
            shard.skipped += dropped;
            files_skipped.fetch_add(dropped, Ordering::Relaxed);
        }
        files
            .into_iter()
            .map(|file| FileEntry {
                abs: file.abs,
                rel: file.rel,
                language: file.language.to_string(),
                class: file.class,
            })
            .collect()
    };

    for file in discovered {
        match std::fs::metadata(&file.abs) {
            Ok(metadata) => {
                if metadata.len() > options.base.max_file_bytes {
                    shard.skipped += 1;
                    files_skipped.fetch_add(1, Ordering::Relaxed);
                    continue;
                }
            }
            Err(source) => {
                if options.compat_v1 {
                    shard.error = Some(CodeGraphError::Index {
                        path: file.abs.display().to_string(),
                        source,
                    });
                    return shard;
                }
                shard.skipped += 1;
                files_skipped.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        }
        let Ok(contents) = std::fs::read_to_string(&file.abs) else {
            shard.skipped += 1;
            files_skipped.fetch_add(1, Ordering::Relaxed);
            continue;
        };
        shard.scanned += 1;
        let total_scanned = files_scanned.fetch_add(1, Ordering::Relaxed) + 1;
        if total_scanned.is_multiple_of(64) {
            tick();
        }

        let file_idx = shard.files.len() as u32;
        let is_config = walk::is_config_language(&file.language);
        // Shell and config tokens collapse to mostly id/lit (no parens on
        // command invocations, key=value lines), so the anchor and diversity
        // floors would silently erase the managed-scaffold/config lanes they
        // exist to surface. Those languages rely on the token floor alone
        // (raised for config).
        let waive_structure = is_config || file.language == "shell";
        let normalized = normalized_lines(&contents);
        shard.files.push(file);
        if normalized.len() < window_lines {
            continue;
        }
        scan_windows(
            &normalized,
            repo_idx as u32,
            file_idx,
            is_config,
            waive_structure,
            window_lines,
            options,
            &mut shard.index,
        );
    }
    shard
}

/// Slide the window over one file's normalized lines, filter, hash, record.
#[allow(clippy::too_many_arguments)]
fn scan_windows(
    normalized: &[NormalizedLine],
    repo_idx: u32,
    file_idx: u32,
    is_config: bool,
    waive_structure: bool,
    window_lines: usize,
    options: &ToolBuildScanOptions,
    index: &mut HashMap<u128, OccList>,
) {
    let mut distinct_scratch: HashSet<&str> = HashSet::new();
    // Prefix sums make per-window token/anchor/import counts O(1).
    let count = normalized.len();
    let mut token_prefix = Vec::with_capacity(count + 1);
    let mut anchor_prefix = Vec::with_capacity(count + 1);
    let mut import_prefix = Vec::with_capacity(count + 1);
    token_prefix.push(0usize);
    anchor_prefix.push(0usize);
    import_prefix.push(0usize);
    for line in normalized {
        token_prefix.push(token_prefix.last().unwrap() + line.token_count);
        anchor_prefix.push(anchor_prefix.last().unwrap() + line.anchor_count);
        import_prefix.push(import_prefix.last().unwrap() + usize::from(line.is_import));
    }

    let min_tokens = if !options.compat_v1 && is_config {
        // Config repetition needs a meaningfully higher bar before it counts.
        options.base.min_normalized_tokens + options.base.min_normalized_tokens / 2
    } else {
        options.base.min_normalized_tokens
    };

    for start in 0..=(count - window_lines) {
        let end = start + window_lines;
        let token_count = token_prefix[end] - token_prefix[start];
        if token_count < min_tokens {
            continue;
        }
        if !options.compat_v1 {
            let import_lines = import_prefix[end] - import_prefix[start];
            if import_lines * 100 > options.max_import_fraction_x100 * window_lines {
                continue;
            }
            if !waive_structure {
                let anchors = anchor_prefix[end] - anchor_prefix[start];
                if anchors < options.min_anchor_tokens {
                    continue;
                }
                if options.min_distinct_tokens > 0 {
                    distinct_scratch.clear();
                    for line in &normalized[start..end] {
                        for token in line.joined.split_whitespace() {
                            distinct_scratch.insert(token);
                        }
                    }
                    if distinct_scratch.len() < options.min_distinct_tokens {
                        continue;
                    }
                }
            }
        }

        let key = window_key(&normalized[start..end]);
        let occ = ScanOccurrence {
            repo_idx,
            file_idx,
            norm_start: start as u32,
            start_line: normalized[start].line_number as u32,
            end_line: normalized[end - 1].line_number as u32,
            token_count: token_count as u32,
        };
        match index.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut entry) => entry.get_mut().push(occ),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(OccList::One(occ));
            }
        }
    }
}

/// First 16 bytes of the v1-identical window hash. The hash feeds each line's
/// pre-joined token string with `\n` separators — byte-identical input to
/// v1's `lines.join("\n")`, with zero per-window string allocation.
pub(crate) fn window_key(window: &[NormalizedLine]) -> u128 {
    let mut hasher = blake3::Hasher::new();
    for (i, line) in window.iter().enumerate() {
        if i > 0 {
            hasher.update(b"\n");
        }
        hasher.update(line.joined.as_bytes());
    }
    let bytes = hasher.finalize();
    u128::from_le_bytes(
        bytes.as_bytes()[..16]
            .try_into()
            .expect("blake3 is 32 bytes"),
    )
}

/// The reconstructed identity of one surviving cluster.
pub(crate) struct ReconstructedWindow {
    pub preview: String,
    pub fingerprint_hex: String,
    pub token_count: usize,
}

/// Re-read just the survivors' head files and rebuild preview text + full
/// fingerprints. Returns `None` for a cluster whose head window no longer
/// hashes to its index key (file changed mid-scan).
fn reconstruct_previews(
    survivors: &[ProtoCluster],
    file_table: &[FileEntry],
    window_lines: usize,
) -> Vec<Option<ReconstructedWindow>> {
    // Group head-window requests by file so each file is read once.
    let mut by_file: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    for (idx, proto) in survivors.iter().enumerate() {
        if let Some(head) = proto.occs.first() {
            by_file.entry(head.file_idx).or_default().push(idx);
        }
    }
    let mut out: Vec<Option<ReconstructedWindow>> = (0..survivors.len()).map(|_| None).collect();
    for (file_idx, proto_indexes) in by_file {
        let Some(file) = file_table.get(file_idx as usize) else {
            continue;
        };
        let Ok(contents) = std::fs::read_to_string(&file.abs) else {
            continue;
        };
        let normalized = normalized_lines(&contents);
        for proto_idx in proto_indexes {
            let proto = &survivors[proto_idx];
            let head = proto.occs[0];
            let start = head.norm_start as usize;
            let end = start + proto.norm_len;
            if end > normalized.len() {
                continue;
            }
            // Verify the head window still hashes to the index key.
            let head_end = start + window_lines;
            if head_end > normalized.len() || window_key(&normalized[start..head_end]) != proto.key
            {
                continue;
            }
            let lines: Vec<&str> = normalized[start..end]
                .iter()
                .map(|line| line.joined.as_str())
                .collect();
            let preview = lines.join("\n");
            let fingerprint_hex = blake3::hash(preview.as_bytes()).to_hex().to_string();
            let token_count = normalized[start..end]
                .iter()
                .map(|line| line.token_count)
                .sum();
            out[proto_idx] = Some(ReconstructedWindow {
                preview,
                fingerprint_hex,
                token_count,
            });
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn build_cluster(
    proto: &ProtoCluster,
    recon: ReconstructedWindow,
    roots: &[(String, PathBuf)],
    file_table: &[FileEntry],
    label_repo_id: &str,
    commit_sha: &str,
    options: &ToolBuildScanOptions,
    window_lines: usize,
) -> ToolBuildCluster {
    let merged = !proto.member_keys.is_empty();
    let occurrence_count = proto.occs.len();

    let mut repos: BTreeSet<&str> = BTreeSet::new();
    let mut files: BTreeSet<String> = BTreeSet::new();
    let mut languages: BTreeMap<&str, usize> = BTreeMap::new();
    let mut total_lines = 0usize;
    let mut token_total = 0usize;
    let mut test_occs = 0usize;
    let mut scaffold_occs = 0usize;
    let mut config_occs = 0usize;
    for occ in &proto.occs {
        let file = &file_table[occ.file_idx as usize];
        repos.insert(roots[occ.repo_idx as usize].0.as_str());
        if options.compat_v1 {
            files.insert(file.rel.clone());
            total_lines += window_lines;
            token_total += occ.token_count as usize;
        } else {
            files.insert(format!("{}:{}", roots[occ.repo_idx as usize].0, file.rel));
            total_lines += (occ.end_line - occ.start_line + 1) as usize;
            token_total += if merged {
                recon.token_count
            } else {
                occ.token_count as usize
            };
        }
        *languages.entry(file.language.as_str()).or_default() += 1;
        match file.class {
            PathClass::Test => test_occs += 1,
            PathClass::ManagedScaffold => scaffold_occs += 1,
            PathClass::Config => config_occs += 1,
            _ => {}
        }
    }

    let language = languages
        .iter()
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
        .map(|(language, _)| (*language).to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let file_count = files.len();

    let mut score = (occurrence_count as u64)
        .saturating_mul(token_total as u64)
        .saturating_add((file_count as u64).saturating_mul(100))
        .saturating_add(total_lines as u64);

    let category = if options.compat_v1 {
        ToolBuildCategory::ToolCandidate
    } else if scaffold_occs * 100 >= options.scaffold_fraction_threshold_x100 * occurrence_count {
        ToolBuildCategory::ManagedScaffold
    } else if config_occs * 100 >= options.scaffold_fraction_threshold_x100 * occurrence_count {
        ToolBuildCategory::ConfigPattern
    } else if test_occs * 100 >= options.test_fraction_threshold_x100 * occurrence_count {
        score /= 4;
        ToolBuildCategory::TestPattern
    } else {
        ToolBuildCategory::ToolCandidate
    };

    let insight = enrich::cluster_insight(
        occurrence_count,
        file_count,
        total_lines,
        &language,
        &recon.preview,
    );

    // v1 keeps the first 12 occurrences (byte parity). v2 caps per repo so
    // every spanning repo stays visible in the compact list even when one
    // repo carries dozens of occurrences.
    let kept: Vec<&ScanOccurrence> = if options.compat_v1 {
        proto.occs.iter().take(12).collect()
    } else {
        // Every repo's first occurrence is always kept; second occurrences
        // fill the remainder up to 64, preserving scan order.
        let mut seen_repos: BTreeSet<u32> = BTreeSet::new();
        let mut kept: Vec<&ScanOccurrence> = proto
            .occs
            .iter()
            .filter(|occ| seen_repos.insert(occ.repo_idx))
            .collect();
        let mut per_repo: BTreeMap<u32, usize> = BTreeMap::new();
        for occ in &proto.occs {
            if kept.len() >= 64 {
                break;
            }
            let extras = per_repo.entry(occ.repo_idx).or_default();
            *extras += 1;
            if *extras == 2 {
                kept.push(occ);
            }
        }
        kept.sort_by_key(|occ| (occ.repo_idx, occ.file_idx, occ.norm_start));
        kept
    };
    let occurrences: Vec<ToolBuildOccurrence> = kept
        .into_iter()
        .map(|occ| {
            let file = &file_table[occ.file_idx as usize];
            ToolBuildOccurrence {
                repo_id: roots[occ.repo_idx as usize].0.clone(),
                commit_sha: commit_sha.to_string(),
                path: file.rel.clone(),
                start_line: occ.start_line as usize,
                end_line: occ.end_line as usize,
                language: file.language.clone(),
                normalized_token_count: if merged {
                    recon.token_count
                } else {
                    occ.token_count as usize
                },
                is_test: file.class == PathClass::Test,
            }
        })
        .collect();

    ToolBuildCluster {
        cluster_id: format!("toolbuild-{}", &recon.fingerprint_hex[..16]),
        repo_id: label_repo_id.to_string(),
        commit_sha: commit_sha.to_string(),
        fingerprint: recon.fingerprint_hex,
        score,
        occurrence_count,
        repo_count: repos.len().max(1),
        file_count,
        total_lines,
        language,
        insight,
        normalized_preview: recon.preview,
        category,
        member_cluster_ids: proto
            .member_keys
            .iter()
            .map(|key| format!("toolbuild-{}", key_hex_prefix(*key)))
            .collect(),
        occurrences,
        ignored: None,
    }
}

/// The first 16 hex chars of the full fingerprint, recovered from the 16-byte
/// index key (which holds the hash's leading bytes).
pub(crate) fn key_hex_prefix(key: u128) -> String {
    let bytes = key.to_le_bytes();
    let mut out = String::with_capacity(16);
    for byte in &bytes[..8] {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}
