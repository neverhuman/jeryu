//! Overlap merging: chain window clusters that are +1-shifted copies of each
//! other into one maximal-span cluster.
//!
//! A duplicated block longer than the window produces a ladder of overlapping
//! window clusters (a 40-line block at window 8 yields 33), each carrying the
//! same occurrence multiset shifted by one *normalized* line. Chaining them
//! restores one honest cluster: real occurrence spans, real LOC counts, one
//! dashboard row instead of 33.
//!
//! The chain test is exact: cluster B succeeds cluster A iff B's occurrence
//! vector equals A's with every `norm_start` advanced by one (same repos, same
//! files, same multiplicity). Where a sub-block is shared by MORE locations
//! than the enclosing block, its occurrence vector differs, the linkage breaks
//! exactly at the boundary, and the more widely duplicated sub-block survives
//! as its own cluster — which is correct, it IS a distinct pattern.

use std::collections::HashMap;

use super::scan::{ProtoCluster, ScanOccurrence};

/// Position identity of an occurrence (everything except line spans/tokens).
type OccPos = (u32, u32, u32); // (repo_idx, file_idx, norm_start)

/// Merge +1-shift chains. `survivors` must be sorted deterministically and
/// every proto's `occs` sorted by (repo, file, norm_start).
pub(crate) fn merge_overlapping(
    survivors: Vec<ProtoCluster>,
    window_lines: usize,
) -> Vec<ProtoCluster> {
    // Exact-keyed position index: no hash-collision handling needed because
    // the key IS the full position vector.
    let mut by_positions: HashMap<Vec<OccPos>, usize> = HashMap::with_capacity(survivors.len());
    for (idx, proto) in survivors.iter().enumerate() {
        by_positions.insert(positions(&proto.occs, 0), idx);
    }

    let mut next: Vec<Option<usize>> = vec![None; survivors.len()];
    let mut prev: Vec<Option<usize>> = vec![None; survivors.len()];
    for (idx, proto) in survivors.iter().enumerate() {
        let successor_key = positions(&proto.occs, 1);
        if let Some(&succ_idx) = by_positions.get(&successor_key)
            && succ_idx != idx
        {
            next[idx] = Some(succ_idx);
            prev[succ_idx] = Some(idx);
        }
    }

    let mut out = Vec::with_capacity(survivors.len());
    let mut consumed = vec![false; survivors.len()];
    for head in 0..survivors.len() {
        if prev[head].is_some() || consumed[head] {
            continue;
        }
        // Walk the chain. `norm_start` sums strictly increase along `next`,
        // so cycles are impossible; the bound is belt-and-braces.
        let mut chain = vec![head];
        let mut cursor = head;
        while let Some(succ) = next[cursor] {
            if consumed[succ] || chain.len() > survivors.len() {
                break;
            }
            consumed[succ] = true;
            chain.push(succ);
            cursor = succ;
        }
        consumed[head] = true;

        if chain.len() == 1 {
            out.push(survivors[head].clone());
            continue;
        }

        let head_proto = &survivors[head];
        let tail_proto = &survivors[*chain.last().expect("chain is non-empty")];
        let occs: Vec<ScanOccurrence> = head_proto
            .occs
            .iter()
            .zip(&tail_proto.occs)
            .map(|(first, last)| ScanOccurrence {
                end_line: last.end_line,
                ..*first
            })
            .collect();
        out.push(ProtoCluster {
            key: head_proto.key,
            occs,
            norm_len: window_lines + chain.len() - 1,
            member_keys: chain.iter().map(|&idx| survivors[idx].key).collect(),
        });
    }
    out
}

fn positions(occs: &[ScanOccurrence], shift: u32) -> Vec<OccPos> {
    occs.iter()
        .map(|occ| (occ.repo_idx, occ.file_idx, occ.norm_start + shift))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn occ(repo: u32, file: u32, norm: u32, start: u32, end: u32) -> ScanOccurrence {
        ScanOccurrence {
            repo_idx: repo,
            file_idx: file,
            norm_start: norm,
            start_line: start,
            end_line: end,
            token_count: 10,
        }
    }

    fn proto(key: u128, occs: Vec<ScanOccurrence>) -> ProtoCluster {
        ProtoCluster {
            key,
            occs,
            norm_len: 8,
            member_keys: Vec::new(),
        }
    }

    #[test]
    fn ladder_collapses_to_one_maximal_cluster() {
        // A 10-normalized-line duplicate at window 8 -> 3 ladder rungs in two
        // files (two repos).
        let survivors = vec![
            proto(1, vec![occ(0, 0, 4, 10, 18), occ(1, 5, 0, 1, 9)]),
            proto(2, vec![occ(0, 0, 5, 11, 19), occ(1, 5, 1, 2, 10)]),
            proto(3, vec![occ(0, 0, 6, 12, 21), occ(1, 5, 2, 3, 12)]),
        ];
        let merged = merge_overlapping(survivors, 8);
        assert_eq!(merged.len(), 1);
        let cluster = &merged[0];
        assert_eq!(cluster.key, 1);
        assert_eq!(cluster.norm_len, 10);
        assert_eq!(cluster.member_keys, vec![1, 2, 3]);
        // Spans run from the head's start to the tail's end, per occurrence.
        assert_eq!(cluster.occs[0].start_line, 10);
        assert_eq!(cluster.occs[0].end_line, 21);
        assert_eq!(cluster.occs[1].start_line, 1);
        assert_eq!(cluster.occs[1].end_line, 12);
    }

    #[test]
    fn multiplicity_boundary_breaks_the_chain() {
        // Window B is shared by a THIRD location, so its occurrence vector
        // differs from A's +1 shift and must survive on its own.
        let survivors = vec![
            proto(1, vec![occ(0, 0, 4, 10, 18), occ(1, 5, 0, 1, 9)]),
            proto(
                2,
                vec![
                    occ(0, 0, 5, 11, 19),
                    occ(1, 5, 1, 2, 10),
                    occ(2, 9, 7, 30, 38),
                ],
            ),
        ];
        let merged = merge_overlapping(survivors, 8);
        assert_eq!(merged.len(), 2);
        assert!(merged.iter().all(|proto| proto.member_keys.is_empty()));
    }

    #[test]
    fn singleton_chain_passes_through_untouched() {
        let original = proto(7, vec![occ(0, 0, 4, 10, 18), occ(1, 5, 0, 1, 9)]);
        let merged = merge_overlapping(vec![original.clone()], 8);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].key, original.key);
        assert!(merged[0].member_keys.is_empty());
        assert_eq!(merged[0].norm_len, 8);
    }
}
