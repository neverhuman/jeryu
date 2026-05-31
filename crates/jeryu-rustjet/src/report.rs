use crate::classifier::AffectedPlan;

#[must_use]
pub fn markdown_summary(plan: &AffectedPlan) -> String {
    let mut out = String::new();
    out.push_str("# RustJet Plan\n\n");
    out.push_str(&format!("- Runner: `{}`\n", plan.runner_class.as_str()));
    out.push_str(&format!("- sccache: `{}`\n", plan.sccache_mode));
    out.push_str(&format!("- fail closed: `{}`\n", plan.fail_closed));
    out.push_str("\n## Affected packages\n\n");
    if plan.affected_packages.is_empty() {
        out.push_str("No package compile/test required.\n");
    } else {
        for package in &plan.affected_packages {
            let reasons = package
                .reasons
                .iter()
                .map(|reason| reason.code())
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("- `{}` — {}\n", package.name, reasons));
        }
    }
    out.push_str("\n## Commands\n\n");
    for command in &plan.commands {
        out.push_str(&format!(
            "- `{}`: `{}`\n",
            command.lane,
            command.argv.join(" ")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classifier::{AffectedPackage, CiCommand, ImpactReason, RunnerClass};
    use std::collections::BTreeSet;

    fn reasons(items: [ImpactReason; 2]) -> BTreeSet<ImpactReason> {
        items.into_iter().collect()
    }

    #[test]
    fn markdown_summary_lists_runner_reasons_and_commands() {
        let plan = AffectedPlan {
            affected_packages: vec![AffectedPackage {
                name: "jeryu-core".to_string(),
                reasons: reasons([
                    ImpactReason::PublicApiChange,
                    ImpactReason::SecuritySensitiveChange,
                ]),
            }],
            reasons: BTreeSet::from([ImpactReason::PublicApiChange]),
            proof_lanes: BTreeSet::from(["public-api".to_string()]),
            commands: vec![CiCommand {
                lane: "api".to_string(),
                argv: vec![
                    "cargo".to_string(),
                    "test".to_string(),
                    "-p".to_string(),
                    "jeryu-core".to_string(),
                ],
            }],
            runner_class: RunnerClass::MicroVmRust,
            sccache_mode: "read-only-source".to_string(),
            fail_closed: true,
        };

        let rendered = markdown_summary(&plan);
        assert!(rendered.contains("- Runner: `microvm-rust`"));
        assert!(rendered.contains("- sccache: `read-only-source`"));
        assert!(rendered.contains("- fail closed: `true`"));
        assert!(rendered.contains("- `jeryu-core` — public-api-change, security-sensitive-change"));
        assert!(rendered.contains("- `api`: `cargo test -p jeryu-core`"));
    }

    #[test]
    fn markdown_summary_marks_docs_only_noop_plan() {
        let plan = AffectedPlan {
            affected_packages: Vec::new(),
            reasons: BTreeSet::from([ImpactReason::DocumentationOnly]),
            proof_lanes: BTreeSet::new(),
            commands: Vec::new(),
            runner_class: RunnerClass::NativeRustHot,
            sccache_mode: "read-write".to_string(),
            fail_closed: false,
        };

        let rendered = markdown_summary(&plan);
        assert!(rendered.contains("No package compile/test required."));
        assert!(rendered.contains("## Commands"));
    }
}
