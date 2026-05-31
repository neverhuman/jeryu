use crate::features::FeatureSelection;
use crate::sharding::ShardPlan;

/// Compute the 1-based shard a test lands in under cargo-nextest's `count`
/// partitioner, given the test's 0-based ordinal in the deterministic listing
/// order and the total shard count.
///
/// cargo-nextest assigns the k-th test (0-based) to partition `(k % N) + 1`.
/// This is the round-robin assignment that guarantees the union of all N
/// partitions equals the full test set exactly once (no test runs twice, none
/// is missed). [`partition_membership`] models the same rule so it can be
/// verified independently of an installed nextest binary.
///
/// # Panics
///
/// Panics if `shard_count` is zero.
#[must_use]
pub fn count_partition_for(test_ordinal: usize, shard_count: usize) -> usize {
    assert!(shard_count > 0, "shard_count must be greater than zero");
    (test_ordinal % shard_count) + 1
}

/// Partition `test_count` ordered tests across `shard_count` shards using the
/// same round-robin rule as cargo-nextest's `count` partitioner.
///
/// Returns a vector of length `shard_count`; entry `i` (0-based) holds the
/// 0-based ordinals assigned to shard `i + 1` (1-based), in ascending order.
///
/// # Panics
///
/// Panics if `shard_count` is zero.
#[must_use]
pub fn partition_membership(test_count: usize, shard_count: usize) -> Vec<Vec<usize>> {
    assert!(shard_count > 0, "shard_count must be greater than zero");
    let mut shards: Vec<Vec<usize>> = vec![Vec::new(); shard_count];
    for ordinal in 0..test_count {
        let shard = count_partition_for(ordinal, shard_count) - 1;
        shards[shard].push(ordinal);
    }
    shards
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NextestCommand {
    pub argv: Vec<String>,
}

impl NextestCommand {
    #[must_use]
    pub fn display(&self) -> String {
        self.argv.join(" ")
    }
}

#[derive(Debug, Clone)]
pub struct NextestPlanner {
    feature_selection: FeatureSelection,
    profile: String,
}

impl Default for NextestPlanner {
    fn default() -> Self {
        Self {
            feature_selection: FeatureSelection::default(),
            profile: "ci".to_string(),
        }
    }
}

impl NextestPlanner {
    #[must_use]
    pub fn new(feature_selection: FeatureSelection) -> Self {
        Self {
            feature_selection,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn command_for_packages(&self, packages: &[String]) -> NextestCommand {
        let mut argv = vec![
            "cargo".to_string(),
            "nextest".to_string(),
            "run".to_string(),
            "--profile".to_string(),
            self.profile.clone(),
        ];
        if packages.is_empty() {
            argv.push("--workspace".to_string());
        } else {
            for package in packages {
                argv.push("--package".to_string());
                argv.push(package.clone());
            }
        }
        argv.extend(self.feature_selection.cargo_args());
        NextestCommand { argv }
    }

    #[must_use]
    pub fn command_for_shard(
        &self,
        packages: &[String],
        shard_plan: &ShardPlan,
        shard_index: usize,
    ) -> NextestCommand {
        let mut command = self.command_for_packages(packages);
        command.argv.push("--partition".to_string());
        // cargo-nextest's count partitioner expects `count:M/N` where M is the
        // 1-based shard index and N is the shard count. The union of all N
        // partitions is exactly the full test set, with no overlap.
        command.argv.push(format!(
            "count:{}/{}",
            shard_index + 1,
            shard_plan.shard_count(),
        ));
        command
    }
}
