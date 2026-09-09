//! Property + unit tests proving that cargo-nextest `count` sharding is an
//! exact cover of the workspace test set: the union of all N shards equals the
//! full test set exactly once — no test runs twice, none is missed.
//!
//! `ops/ci/shard.sh i N` runs shard `i` of `N` via `nextest --partition
//! count:(i+1)/N`. [`jeryu_rustjet::partition_membership`] models that exact
//! round-robin assignment, so these tests pin the invariant the shell driver
//! relies on without needing an installed nextest binary.

use jeryu_rustjet::{count_partition_for, partition_membership};
use proptest::prelude::*;
use std::collections::BTreeSet;

/// Core invariant: for `test_count` tests across `shard_count` shards, the
/// multiset union of all shards equals `{0, 1, ..., test_count-1}` with every
/// ordinal appearing exactly once.
fn assert_exact_cover(test_count: usize, shard_count: usize) {
    let shards = partition_membership(test_count, shard_count);

    assert_eq!(
        shards.len(),
        shard_count,
        "partition_membership must return one bucket per shard"
    );

    // 1. Total assigned == total tests (nothing dropped, nothing duplicated in
    //    aggregate count).
    let total_assigned: usize = shards.iter().map(Vec::len).sum();
    assert_eq!(
        total_assigned, test_count,
        "sum of shard sizes ({total_assigned}) must equal the full test count ({test_count})"
    );

    // 2. The union (as a set) is exactly the full ordinal range, and because the
    //    union set size equals the summed length, there are no duplicates.
    let mut union: BTreeSet<usize> = BTreeSet::new();
    for shard in &shards {
        for &ordinal in shard {
            assert!(
                union.insert(ordinal),
                "ordinal {ordinal} assigned to more than one shard"
            );
        }
    }
    let expected: BTreeSet<usize> = (0..test_count).collect();
    assert_eq!(
        union, expected,
        "union of shards must equal the full test set exactly once"
    );

    // 3. Cross-check each ordinal against the single-test assignment helper, so
    //    the bulk and per-test models agree.
    for (shard_idx, shard) in shards.iter().enumerate() {
        for &ordinal in shard {
            assert_eq!(
                count_partition_for(ordinal, shard_count),
                shard_idx + 1,
                "ordinal {ordinal} membership disagrees with count_partition_for"
            );
        }
    }
}

#[test]
fn exact_cover_for_representative_shard_counts() {
    // A few concrete (test_count, shard_count) pairs, including the canonical
    // `shard.sh 0 4` shape and edge cases (N=1, more shards than tests).
    for &(tests, shards) in &[
        (0usize, 1usize),
        (0, 4),
        (1, 1),
        (1, 4),
        (18, 4),   // jeryu-runner-core's real test count under count:i/4
        (1272, 4), // full workspace test count under count:i/4
        (7, 3),
        (10, 10),
        (10, 16), // more shards than tests -> some shards empty, still exact
        (100, 8),
    ] {
        assert_exact_cover(tests, shards);
    }
}

#[test]
fn single_shard_holds_every_test() {
    let shards = partition_membership(25, 1);
    assert_eq!(shards.len(), 1);
    assert_eq!(shards[0], (0..25).collect::<Vec<_>>());
}

#[test]
fn round_robin_assignment_is_contiguous_modulo() {
    // Shard 1 (count:1/4) gets ordinals 0,4,8,...; shard 2 gets 1,5,9,...; etc.
    let shards = partition_membership(12, 4);
    assert_eq!(shards[0], vec![0, 4, 8]);
    assert_eq!(shards[1], vec![1, 5, 9]);
    assert_eq!(shards[2], vec![2, 6, 10]);
    assert_eq!(shards[3], vec![3, 7, 11]);
}

proptest! {
    /// For arbitrary test counts and shard counts, sharding is always an exact
    /// cover of the full set.
    #[test]
    fn prop_shards_are_an_exact_cover(
        test_count in 0usize..2_000,
        shard_count in 1usize..64,
    ) {
        assert_exact_cover(test_count, shard_count);
    }

    /// Shard sizes are balanced: every shard differs by at most one test, which
    /// is what makes `shard.sh` give roughly equal wall-clock per shard.
    #[test]
    fn prop_shard_sizes_are_balanced(
        test_count in 0usize..2_000,
        shard_count in 1usize..64,
    ) {
        let shards = partition_membership(test_count, shard_count);
        let min = shards.iter().map(Vec::len).min().unwrap_or(0);
        let max = shards.iter().map(Vec::len).max().unwrap_or(0);
        prop_assert!(
            max - min <= 1,
            "shard sizes must differ by at most one (min={min}, max={max})"
        );
    }
}
