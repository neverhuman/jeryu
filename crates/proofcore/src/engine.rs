//! Proof planning and witness verification.

use crate::matcher::PathPattern;
use crate::policy::{
    ChangeSet, GeneratedZone, OwnerRule, ProofBlocker, ProofEvidence, ProofLane, ProofPlan,
    TestRule,
};
use forge_core::phase7::{ProofWitness, Receipt, ReceiptKind};
use std::collections::{BTreeMap, BTreeSet};

/// Jankurai proof engine.
#[derive(Clone, Debug)]
pub struct ProofEngine {
    owner_rules: Vec<OwnerRule>,
    test_rules: Vec<TestRule>,
    generated_zones: Vec<GeneratedZone>,
    lanes: BTreeMap<String, ProofLane>,
}

impl ProofEngine {
    /// Creates a proof engine.
    pub fn new(
        owner_rules: Vec<OwnerRule>,
        test_rules: Vec<TestRule>,
        generated_zones: Vec<GeneratedZone>,
        lanes: Vec<ProofLane>,
    ) -> Self {
        let lanes = lanes
            .into_iter()
            .map(|lane| (lane.name.clone(), lane))
            .collect();
        Self {
            owner_rules,
            test_rules,
            generated_zones,
            lanes,
        }
    }

    /// Builds a proof plan or returns the first hard blocker.
    pub fn plan(&self, change_set: &ChangeSet) -> Result<ProofPlan, ProofBlocker> {
        let mut owners = BTreeSet::new();
        let mut lane_names = BTreeSet::new();
        let mut paths = Vec::new();

        for changed in &change_set.paths {
            if let Some(zone) = self
                .generated_zones
                .iter()
                .find(|zone| zone.pattern.matches(&changed.path))
            {
                if change_set.agent_authored && !zone.allow_agent_edits {
                    return Err(ProofBlocker::GeneratedZoneEditDenied {
                        path: changed.path.clone(),
                        reason: zone.reason.clone(),
                    });
                }
            }

            let path_owners: Vec<String> = self
                .owner_rules
                .iter()
                .filter(|rule| rule.pattern.matches(&changed.path))
                .flat_map(|rule| rule.owners.clone())
                .collect();
            if path_owners.is_empty() {
                return Err(ProofBlocker::OwnerlessPath(changed.path.clone()));
            }
            owners.extend(path_owners);

            let path_lanes: Vec<String> = self
                .test_rules
                .iter()
                .filter(|rule| rule.pattern.matches(&changed.path))
                .flat_map(|rule| rule.lanes.clone())
                .collect();
            if path_lanes.is_empty() {
                return Err(ProofBlocker::UnmappedProofLane(changed.path.clone()));
            }
            lane_names.extend(path_lanes);
            if changed.sensitive {
                lane_names.insert("policy".to_string());
            }
            paths.push(changed.path.clone());
        }

        let mut lanes = Vec::new();
        for lane_name in lane_names {
            match self.lanes.get(&lane_name) {
                Some(lane) => lanes.push(lane.clone()),
                None => return Err(ProofBlocker::UnmappedProofLane(lane_name)),
            }
        }

        Ok(ProofPlan {
            repo: change_set.repo.clone(),
            pr: change_set.pr.clone(),
            head_sha: change_set.head_sha.clone(),
            paths,
            owners: owners.into_iter().collect(),
            lanes,
        })
    }

    /// Verifies lane evidence and emits a proof witness.
    pub fn verify(
        &self,
        plan: &ProofPlan,
        evidence: &[ProofEvidence],
    ) -> Result<ProofWitness, ProofBlocker> {
        let evidence_by_lane: BTreeMap<&str, &ProofEvidence> = evidence
            .iter()
            .map(|entry| (entry.lane.as_str(), entry))
            .collect();

        for lane in &plan.lanes {
            if !lane.required {
                continue;
            }
            let Some(entry) = evidence_by_lane.get(lane.name.as_str()) else {
                return Err(ProofBlocker::MissingEvidence(lane.name.clone()));
            };
            if !entry.success {
                return Err(ProofBlocker::FailedLane(lane.name.clone()));
            }
        }

        let lane_names = plan
            .lanes
            .iter()
            .map(|lane| lane.name.clone())
            .collect::<Vec<_>>();
        let receipt = Receipt::new(
            ReceiptKind::ProofWitness,
            plan.repo.clone(),
            None,
            plan.pr.to_string(),
            plan.head_sha.clone(),
            format!(
                "proof witness covers {} path(s) and {} lane(s)",
                plan.paths.len(),
                lane_names.len()
            ),
            lane_names
                .iter()
                .map(|lane| format!("proof-lane:{lane}"))
                .collect(),
            "none: all required lanes passed",
        );

        Ok(ProofWitness {
            id: receipt.id.clone(),
            repo: plan.repo.clone(),
            pr: plan.pr.clone(),
            head_sha: plan.head_sha.clone(),
            changed_paths: plan.paths.clone(),
            lanes: lane_names,
            owners: plan.owners.clone(),
            receipt,
        })
    }
}

/// Returns the Phase 7 default engine matching the archive file tree.
pub fn default_phase7_engine() -> ProofEngine {
    let owner_rules = vec![
        owner("crates/forge-core/**", &["@forge-core"]),
        owner("crates/proofcore/**", &["@proofcore"]),
        owner("crates/ci-scheduler/**", &["@scheduler"]),
        owner("crates/agentbridge/**", &["@agentbridge"]),
        owner("crates/phase7-cli/**", &["@phase7"]),
        owner("agent/**", &["@jankurai"]),
        owner("ops/ci/**", &["@ops"]),
        owner("scripts/**", &["@ops"]),
        owner("docs/**", &["@docs", "@phase7"]),
        owner("Cargo.toml", &["@phase7"]),
        owner("rust-toolchain.toml", &["@phase7"]),
        owner("Justfile", &["@ops", "@phase7"]),
        owner("AGENTS.md", &["@jankurai"]),
    ];
    let test_rules = vec![
        test("crates/forge-core/**", &["unit", "policy"]),
        test("crates/proofcore/**", &["unit", "jankurai-proof"]),
        test("crates/ci-scheduler/**", &["unit", "merge-queue-sim"]),
        test("crates/agentbridge/**", &["unit", "agent-safety"]),
        test("agent/**", &["jankurai-proof", "policy"]),
        test("ops/ci/**", &["local-ci-parity"]),
        test("scripts/**", &["local-ci-parity"]),
        test("docs/**", &["docs"]),
        test("Cargo.toml", &["unit", "local-ci-parity"]),
        test("rust-toolchain.toml", &["unit", "local-ci-parity"]),
        test("Justfile", &["local-ci-parity"]),
        test("AGENTS.md", &["jankurai-proof"]),
    ];
    let generated_zones = vec![GeneratedZone {
        pattern: PathPattern::new("target/**"),
        allow_agent_edits: false,
        reason: "Build outputs are generated.".to_string(),
    }];
    let lanes = vec![
        lane("unit", &["cargo test --workspace"], true),
        lane("jankurai-proof", &["cargo test -p proofcore"], true),
        lane(
            "merge-queue-sim",
            &["cargo test -p ci-scheduler concurrent"],
            true,
        ),
        lane("agent-safety", &["cargo test -p agentbridge"], true),
        lane("local-ci-parity", &["./scripts/ci-local.sh"], true),
        lane("policy", &["cargo test --workspace policy"], true),
        lane("docs", &["cargo test --workspace docs"], false),
    ];
    ProofEngine::new(owner_rules, test_rules, generated_zones, lanes)
}

fn owner(pattern: &str, owners: &[&str]) -> OwnerRule {
    OwnerRule {
        pattern: PathPattern::new(pattern),
        owners: owners.iter().map(|owner| (*owner).to_string()).collect(),
    }
}

fn test(pattern: &str, lanes: &[&str]) -> TestRule {
    TestRule {
        pattern: PathPattern::new(pattern),
        lanes: lanes.iter().map(|lane| (*lane).to_string()).collect(),
    }
}

fn lane(name: &str, commands: &[&str], required: bool) -> ProofLane {
    ProofLane {
        name: name.to_string(),
        commands: commands
            .iter()
            .map(|command| (*command).to_string())
            .collect(),
        required,
    }
}

#[cfg(test)]
mod tests {
    use super::default_phase7_engine;
    use crate::policy::{ChangeSet, ProofBlocker, ProofEvidence};
    use forge_core::phase7::{ChangedPath, PullRequestId, RepoId};

    fn changes(paths: Vec<ChangedPath>) -> ChangeSet {
        ChangeSet {
            repo: RepoId::new("repo_phase7"),
            pr: PullRequestId::new("pr_1"),
            head_sha: "head001".to_string(),
            paths,
            agent_authored: false,
        }
    }

    #[test]
    fn ownerless_path_blocks_merge() {
        let engine = default_phase7_engine();
        let err = engine
            .plan(&changes(vec![ChangedPath::new("unknown/path.rs")]))
            .expect_err("unknown paths must be blocked");
        assert_eq!(
            err,
            ProofBlocker::OwnerlessPath("unknown/path.rs".to_string())
        );
    }

    #[test]
    fn unmapped_proof_lane_blocks_merge() {
        let engine = default_phase7_engine();
        let custom_engine = crate::engine::ProofEngine::new(
            vec![crate::policy::OwnerRule {
                pattern: crate::matcher::PathPattern::new("custom/**"),
                owners: vec!["@custom".to_string()],
            }],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );
        let err = custom_engine
            .plan(&changes(vec![ChangedPath::new("custom/lib.rs")]))
            .expect_err("owned but unmapped paths must be blocked");
        assert_eq!(
            err,
            ProofBlocker::UnmappedProofLane("custom/lib.rs".to_string())
        );
        let _ = engine;
    }

    #[test]
    fn generated_zone_blocks_agent_edit() {
        let engine = default_phase7_engine();
        let mut change_set = changes(vec![ChangedPath::new("target/debug/build.out")]);
        change_set.agent_authored = true;
        let err = engine
            .plan(&change_set)
            .expect_err("agent edits to generated zones must be blocked");
        match err {
            ProofBlocker::GeneratedZoneEditDenied { path, .. } => {
                assert_eq!(path, "target/debug/build.out")
            }
            other => panic!("unexpected blocker: {other:?}"),
        }
    }

    #[test]
    fn proof_witness_is_minted_when_required_lanes_pass() {
        let engine = default_phase7_engine();
        let plan = engine
            .plan(&changes(vec![ChangedPath::new(
                "crates/proofcore/src/lib.rs",
            )]))
            .expect("proofcore paths have owners and lanes");
        let evidence = plan
            .lanes
            .iter()
            .map(|lane| ProofEvidence {
                lane: lane.name.clone(),
                commands: lane.commands.clone(),
                success: true,
                log_digest: format!("digest:{}", lane.name),
            })
            .collect::<Vec<_>>();
        let witness = engine
            .verify(&plan, &evidence)
            .expect("passing evidence creates witness");
        assert_eq!(witness.head_sha, "head001");
        assert!(witness.lanes.contains(&"unit".to_string()));
    }
}
