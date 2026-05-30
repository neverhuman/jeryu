use crate::features::FeatureSelection;
use crate::sharding::ShardPlan;

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
        command.argv.push(format!(
            "count:{}:{}",
            shard_plan.shard_count(),
            shard_index + 1
        ));
        command
    }
}
