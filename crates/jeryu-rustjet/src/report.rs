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
